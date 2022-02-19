use std::process::{Command, Stdio, ChildStdout};
use std::collections::HashMap;
use std::error::Error;
use std::io::{Read, Seek};
use std::path::{Path};
use std::ffi::OsStr;
use std::fs::File;
use std::convert::AsRef;

use maplit::hashmap;
use tempfile::tempfile;

use crate::config;
use crate::shell;

pub struct Native {
    pub config: config::Container,
    pub cwd: Option<String>,
    pub envs: HashMap<String, String>,
}
pub struct CaptureTarget {
    pub stdout: File,
    pub stderr: File,
}
impl CaptureTarget {
    pub fn read_stdout(&mut self, buf: &mut String) -> Result<usize, std::io::Error> {
        self.stdout.seek(std::io::SeekFrom::Start(0))?;
        return self.stdout.read_to_string(buf);
    }
    pub fn read_stderr(&mut self, buf: &mut String) -> Result<usize, std::io::Error> {
        self.stderr.seek(std::io::SeekFrom::Start(0))?;
        return self.stderr.read_to_string(buf);
    }
}
impl<'a> shell::Shell for Native {
    fn new(config: &config::Container) -> Self {
        return Native {
            config: config.clone(),
            cwd: None,
            envs: hashmap!{}
        }
    }
    fn set_cwd<P: AsRef<Path>>(&mut self, dir: &Option<P>) -> Result<(), Box<dyn Error>> {
        self.cwd = match dir {
            Some(d) => Some(d.as_ref().to_str().unwrap().to_string()),
            None => None
        };
        Ok(())
    }
    fn set_env(&mut self, key: &str, val: String) -> Result<(), Box<dyn Error>> {
        let ent = self.envs.entry(key.to_string());
        ent.and_modify(|e| *e = val.clone()).or_insert(val);
        Ok(())
    }
    fn config(&self) -> &config::Container {
        return &self.config;
    }
    fn output_of<I, K, V, P>(
        &self, args: &Vec<&str>, envs: I, cwd: &Option<P>
    ) -> Result<String, shell::ShellError> 
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path> {
        let (mut cmd, mut ct) = self.create_command(
            args, envs, cwd, &shell::Settings { capture: true, interactive: false, silent: false}
        );
        return Native::get_output(&mut cmd, &mut ct);
    }
    fn exec<I, K, V, P>(
        &self, args: &Vec<&str>, envs: I, cwd: &Option<P>, settings: &shell::Settings
    ) -> Result<String, shell::ShellError> 
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path> {
        let config = self.config.borrow();
        if config.runtime.dryrun {
            let cmd = args.join(" ");
            println!("dryrun: {}", cmd);
            return Ok(cmd);
        } else if !settings.interactive && settings.silent {
            // regardless for the value of `capture`, always capture value
            let (mut cmd, mut ct) = self.create_command(
                args, envs, cwd, &shell::Settings { capture: true, interactive: false, silent: false }
            );
            return Native::run_as_child(&mut cmd, &mut ct);
        } else {
            let (mut cmd, mut ct) = self.create_command(args, envs, cwd, settings);
            return Native::run_as_child(&mut cmd, &mut ct);
        }
    }
}
impl Native {
    fn create_command<I, K, V, P>(
        &self, args: &Vec<&str>, envs: I, cwd: &Option<P>, settings: &shell::Settings
    ) -> (Command, Option<CaptureTarget>)
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path> {
        let mut c = Command::new(args[0]);
        c.args(&args[1..]);
        c.envs(envs);
        let cwd_used = match cwd {
            Some(d) => {
                c.current_dir(d.as_ref()); 
                d.as_ref().to_string_lossy().to_string()
            },
            None => match &self.cwd {
                Some(cwd) => {
                    c.current_dir(cwd); 
                    cwd.clone()
                },
                _ => "no cwd".to_string()
            }
        };
        c.envs(&self.envs);
        log::trace!(
            "create_command:[{}]@[{}] envs[{}]", 
            args.join(" "), cwd_used,
            c.get_envs().collect::<Vec<(&OsStr, Option<&OsStr>)>>().iter().map(
                |(k,v)| format!(
                    "{}={}", k.to_string_lossy(), 
                    v.map(|s| s.to_string_lossy().to_string()).unwrap_or("".to_string())
                )
            ).collect::<Vec<String>>().join(",")
        );
        let ct = if settings.capture {
            // windows std::process::Command does not work well with huge (>1kb) output piping.
            // see https://github.com/rust-lang/rust/issues/45572 for detail
            if cfg!(windows) {
                let v = CaptureTarget {
                    stdout: tempfile().unwrap(),
                    stderr: tempfile().unwrap(),
                };
                log::debug!("capture to temp file {:?} {:?}", v.stdout, v.stderr);
                c.stdout(Stdio::from(v.stdout.try_clone().unwrap()));
                c.stderr(Stdio::from(v.stderr.try_clone().unwrap()));
                Some(v)
            } else {
                c.stdout(Stdio::piped());
                c.stderr(Stdio::piped());
                None
            }
        } else {
            None
        };
        return (c, ct);
    }
    fn get_output(cmd: &mut Command, ct: &mut Option<CaptureTarget>) -> Result<String, shell::ShellError> {
        // TODO: option to capture stderr as no error case
        match cmd.output() {
            Ok(output) => {
                if output.status.success() { 
                    match ct {
                        Some(v) => {
                            let mut buf = String::new();
                            v.read_stdout(&mut buf).map_err(|e| shell::ShellError::OtherFailure{
                                cause: format!("cannot read from stdout tempfile error {:?}", e),
                                cmd: format!("{:?}", cmd)
                            })?;
                            log::debug!("stdout: [{}]", buf);
                            return Ok(buf.trim().to_string());
                        },
                        None => match String::from_utf8(output.stdout) {
                            Ok(s) => return Ok(s.trim().to_string()),
                            Err(err) => return Err(shell::ShellError::OtherFailure{
                                cause: format!("stdout character code error {:?}", err),
                                cmd: format!("{:?}", cmd)
                            })
                        }
                    }
                } else {
                    match ct {
                        Some(v) => {
                            let mut buf = String::new();
                            v.read_stderr(&mut buf).map_err(|e| shell::ShellError::OtherFailure{
                                cause: format!("cannot read from stderr tempfile error {:?}", e),
                                cmd: format!("{:?}", cmd)
                            })?;
                            return Ok(buf.trim().to_string());
                        },
                        None => match String::from_utf8(output.stderr) {
                            Ok(s) => return Err(shell::ShellError::OtherFailure{ 
                                cause: format!("command returns error {}", s),
                                cmd: format!("{:?}", cmd)
                            }),
                            Err(err) => return Err(shell::ShellError::OtherFailure{
                                cause: format!("stderr character code error {:?}", err),
                                cmd: format!("{:?}", cmd)
                            })
                        }
                    }
                }
            },
            Err(err) => return Err(shell::ShellError::OtherFailure{
                cause: format!("get output error {:?}", err),
                cmd: format!("{:?}", cmd)
            })
        }
    }
    fn read_stdout_or_empty(cmd: &Command, stdout: Option<ChildStdout>, ct: &mut Option<CaptureTarget>) -> Result<String, shell::ShellError> {
        let mut buf = String::new();
        match ct {
            Some(v) => {
                let mut buf = String::new();
                v.read_stdout(&mut buf).map_err(|e| shell::ShellError::OtherFailure{
                    cause: format!("cannot read from stderr tempfile error {:?}", e),
                    cmd: format!("{:?}", cmd)
                })?;
            },
            None => match stdout {
                Some(mut stream) => {
                    match stream.read_to_string(&mut buf) {
                        Ok(_) => {},
                        Err(err) => {
                            log::error!("read_stdout_or_empty error: {:?}", err);
                        }
                    }
                },
                None => {}
            }
        }
        Ok(buf)
    }
    fn run_as_child(cmd: &mut Command, ct: &mut Option<CaptureTarget>) -> Result<String,shell::ShellError> {
        match cmd.spawn() {
            Ok(mut process) => {
                match process.wait() { 
                    Ok(status) => {
                        if status.success() {
                            let mut s = String::new();
                            match ct {
                                Some(v) => {
                                    let mut buf = String::new();
                                    v.read_stdout(&mut buf).map_err(|e| shell::ShellError::OtherFailure{
                                        cause: format!("cannot read from stderr tempfile error {:?}", e),
                                        cmd: format!("{:?}", cmd)
                                    })?;
                                    log::debug!("stdout: [{}]", buf);
                                    return Ok(buf.trim().to_string())
                                },
                                None => match process.stdout {
                                    Some(mut stream) => match stream.read_to_string(&mut s) {
                                        Ok(_) => return Ok(s.trim().to_string()),
                                        Err(err) => return Err(shell::ShellError::OtherFailure{
                                            cause: format!("read stream error {:?}", err),
                                            cmd: format!("{:?}", cmd)
                                        })
                                    },
                                    None => return Ok("".to_string())
                                }
                            }
                        } else {
                            let mut s = String::new();
                            let output = match process.stderr {
                                Some(mut stream) => {
                                    match stream.read_to_string(&mut s) {
                                        Ok(_) => if s.is_empty() { Self::read_stdout_or_empty(&cmd, process.stdout, ct)? } else { s },
                                        Err(_) => Self::read_stdout_or_empty(&cmd, process.stdout, ct)?
                                    }
                                },
                                None => Self::read_stdout_or_empty(&cmd, process.stdout, ct)?
                            };           
                            return match status.code() {
                                Some(_) => Err(shell::ShellError::ExitStatus{ 
                                    status, stderr: output,
                                    cmd: format!("{:?}", cmd)
                                }),
                                None => Err(shell::ShellError::OtherFailure{
                                    cause: if output.is_empty() { 
                                        format!("cmd terminated by signal")
                                    } else {
                                        format!("cmd failed. output: {}", output)
                                    },
                                    cmd: format!("{:?}", cmd)
                                }),
                            }
                        }
                    },
                    Err(err) => Err(shell::ShellError::OtherFailure{
                        cause: format!("wait process error {:?}", err),
                        cmd: format!("{:?}", cmd)
                    })
                }
            },
            Err(err) => Err(shell::ShellError::OtherFailure{
                cause: format!("process spawn error {:?}", err),
                cmd: format!("{:?}", cmd)
            })
        }
    }
}
