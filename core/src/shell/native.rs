use std::process::{Command, Stdio, ChildStdout};
use std::collections::HashMap;
use std::borrow::Cow;
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
struct CaptureTarget {
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
    fn set_cwd<P: shell::ArgTrait>(&mut self, dir: &Option<P>) -> Result<(), Box<dyn Error>> {
        self.cwd = match dir {
            Some(d) => Some(d.value()),
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
    fn output_of<'b, I, J, K, P>(
        &self, args: I, envs: J, cwd: &Option<P>
    ) -> Result<String, shell::ShellError> 
    where 
    I: IntoIterator<Item = shell::Arg<'b>>,
    J: IntoIterator<Item = (K, shell::Arg<'b>)>,
    K: AsRef<OsStr>, P: shell::ArgTrait
    {
        let (mut cmd, mut ct, cmdstr) = self.create_command(
            args, envs, cwd, &shell::capture()
        );
        return Native::get_output(&mut cmd, &mut ct, cmdstr);
    }
    fn exec<'b, I, J, K, P>(
        &self, args: I, envs: J, cwd: &Option<P>, settings: &shell::Settings
    ) -> Result<String, shell::ShellError> 
    where 
        I: IntoIterator<Item = shell::Arg<'b>>,
        J: IntoIterator<Item = (K, shell::Arg<'b>)>,
        K: AsRef<OsStr>, P: shell::ArgTrait
    {
        if !settings.interactive && settings.silent {
            // regardless for the value of `capture`, always capture value
            let mut adjusted_settings = shell::capture();
            let (mut cmd, mut ct, cmdstr) = self.create_command(
                args, envs, cwd, match &settings.paths {
                    Some(paths) => adjusted_settings.paths(paths.clone()),
                    None => &mut adjusted_settings
                }
            );
            return Native::run_as_child(&mut cmd, &mut ct, cmdstr);
        } else {
            let (mut cmd, mut ct, cmdstr) = self.create_command(args, envs, cwd, settings);
            return Native::run_as_child(&mut cmd, &mut ct, cmdstr);
        }
    }
}
impl Native {
    fn add_paths<'a>(envs: &mut HashMap<String, Box<dyn shell::ArgTrait + 'a>>, paths: &Option<Vec<String>>) {
        envs.insert("PATH".to_string(), match paths {
            Some(paths) => shell::arg!(format!("{}:{}", match envs.get("PATH") {
                Some(v) => v.value(),
                None => match std::env::var("PATH") {
                    Ok(v) => v,
                    Err(e) => panic!("fail to get system PATH by {:?}", e)
                }
            }, paths.join(":"))),
            None => shell::arg!(match std::env::var("PATH") {
                Ok(v) => v,
                Err(e) => panic!("fail to get system PATH by {:?}", e)
            })
        });
    }
    fn create_command<'a, I, J, K, P>(
        &self, args: I, envs: J, cwd: &Option<P>, settings: &shell::Settings
    ) -> (Command, Option<CaptureTarget>, String)
    where
        I: IntoIterator<Item = shell::Arg<'a>>,
        J: IntoIterator<Item = (K, shell::Arg<'a>)>,
        K: AsRef<OsStr>, P: shell::ArgTrait
    {
        let mut args_vec = vec![];
        let mut raw_args = vec![];
        for a in args.into_iter() {
            raw_args.push(a.value());
            args_vec.push(a);
        }
        let mut envs_map = hashmap!{};
        for (k,v) in envs.into_iter() {
            let key = k.as_ref().to_string_lossy().to_string();
            envs_map.insert(key, v);
        }
        Self::add_paths(&mut envs_map, &settings.paths);
        let mut c = Command::new(&raw_args[0]);
        if !settings.env_inherit {
            c.env_clear();
        }
        c.args(&raw_args[1..]);
        c.envs(&self.envs);
        c.envs(envs_map.iter().map(|(k,v)| (k, v.value())));
        let cwd_used = match cwd {
            Some(d) => {
                c.current_dir(&Path::new(&d.value())); 
                d.view()
            },
            None => match &self.cwd {
                Some(cwd) => {
                    c.current_dir(cwd);
                    Cow::Owned(cwd.clone())
                },
                _ => Cow::Borrowed("none")
            }
        };
        let cmdstr = format!(
            "[{}] cwd[{}] envs[{}]",
            args_vec.iter().map(|a| a.view()).collect::<Vec<Cow<'_, str>>>().join(" "), cwd_used,
            envs_map.iter().map(|(k,v)| format!("{}={}", k, v.view())).collect::<Vec<String>>().join(",")
        );
        log::trace!("create_command:{}", cmdstr);
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
        return (c, ct, cmdstr);
    }
    fn get_output(cmd: &mut Command, ct: &mut Option<CaptureTarget>, cmdstr: String) -> Result<String, shell::ShellError> {
        // TODO: option to capture stderr as no error case
        match cmd.output() {
            Ok(output) => {
                if output.status.success() { 
                    match ct {
                        Some(v) => {
                            let mut buf = String::new();
                            v.read_stdout(&mut buf).map_err(|e| shell::ShellError::OtherFailure{
                                cause: format!("cannot read from stdout tempfile error {:?}", e),
                                cmd: cmdstr
                            })?;
                            log::debug!("stdout: [{}]", buf);
                            return Ok(buf.trim().to_string());
                        },
                        None => match String::from_utf8(output.stdout) {
                            Ok(s) => return Ok(s.trim().to_string()),
                            Err(err) => return Err(shell::ShellError::OtherFailure{
                                cause: format!("stdout character code error {:?}", err),
                                cmd: cmdstr
                            })
                        }
                    }
                } else {
                    match ct {
                        Some(v) => {
                            let mut buf = String::new();
                            v.read_stderr(&mut buf).map_err(|e| shell::ShellError::OtherFailure{
                                cause: format!("cannot read from stderr tempfile error {:?}", e),
                                cmd: cmdstr
                            })?;
                            return Ok(buf.trim().to_string());
                        },
                        None => match String::from_utf8(output.stderr) {
                            Ok(s) => return Err(shell::ShellError::OtherFailure{ 
                                cause: format!("command returns error {}", s),
                                cmd: cmdstr
                            }),
                            Err(err) => return Err(shell::ShellError::OtherFailure{
                                cause: format!("stderr character code error {:?}", err),
                                cmd: cmdstr
                            })
                        }
                    }
                }
            },
            Err(err) => return Err(shell::ShellError::OtherFailure{
                cause: format!("get output error {:?}", err),
                cmd: cmdstr
            })
        }
    }
    fn read_stdout_or_empty(
        cmd: &Command, stdout: Option<ChildStdout>, ct: &mut Option<CaptureTarget>, cmdstr: &str
    ) -> Result<String, shell::ShellError> {
        let mut buf = String::new();
        match ct {
            Some(v) => {
                let mut buf = String::new();
                v.read_stdout(&mut buf).map_err(|e| shell::ShellError::OtherFailure{
                    cause: format!("cannot read from stderr tempfile error {:?}", e),
                    cmd: cmdstr.to_string()
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
    fn run_as_child(cmd: &mut Command, ct: &mut Option<CaptureTarget>, cmdstr: String) -> Result<String,shell::ShellError> {
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
                                        cmd: cmdstr
                                    })?;
                                    log::debug!("stdout: [{}]", buf);
                                    return Ok(buf.trim().to_string())
                                },
                                None => match process.stdout {
                                    Some(mut stream) => match stream.read_to_string(&mut s) {
                                        Ok(_) => return Ok(s.trim().to_string()),
                                        Err(err) => return Err(shell::ShellError::OtherFailure{
                                            cause: format!("read stream error {:?}", err),
                                            cmd: cmdstr
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
                                        Ok(_) => if s.is_empty() {
                                            Self::read_stdout_or_empty(&cmd, process.stdout, ct, &cmdstr)?
                                        } else {
                                            s
                                        },
                                        Err(_) => Self::read_stdout_or_empty(&cmd, process.stdout, ct, &cmdstr)?
                                    }
                                },
                                None => Self::read_stdout_or_empty(&cmd, process.stdout, ct, &cmdstr)?
                            };           
                            return match status.code() {
                                Some(_) => Err(shell::ShellError::ExitStatus{ 
                                    status, stderr: output,
                                    cmd: cmdstr
                                }),
                                None => Err(shell::ShellError::OtherFailure{
                                    cause: if output.is_empty() { 
                                        format!("cmd terminated by signal")
                                    } else {
                                        format!("cmd failed. output: {}", output)
                                    },
                                    cmd: cmdstr
                                }),
                            }
                        }
                    },
                    Err(err) => Err(shell::ShellError::OtherFailure{
                        cause: format!("wait process error {:?}", err),
                        cmd: cmdstr
                    })
                }
            },
            Err(err) => Err(shell::ShellError::OtherFailure{
                cause: format!("process spawn error {:?}", err),
                cmd: cmdstr
            })
        }
    }
}
