use std::process::{Command, Stdio, ChildStdout};
use std::collections::HashMap;
use std::error::Error;
use std::io::Read;
use std::path::Path;
use std::ffi::OsStr;
use std::convert::AsRef;

use maplit::hashmap;

use crate::config;
use crate::shell;

pub struct Native {
    pub config: config::Container,
    pub cwd: Option<String>,
    pub envs: HashMap<String, String>,
}
impl<'a> shell::Shell for Native {
    fn new(config: &config::Container) -> Self {
        return Native {
            config: config.clone(),
            cwd: None,
            envs: hashmap!{}
        }
    }
    fn set_cwd<P: AsRef<Path>>(&mut self, dir: Option<&P>) -> Result<(), Box<dyn Error>> {
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
        &self, args: &Vec<&str>, envs: I, cwd: Option<&P>
    ) -> Result<String, shell::ShellError> 
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path> {
        let mut cmd = self.create_command(args, envs, cwd, true);
        return Native::get_output(&mut cmd);
    }
    fn exec<I, K, V, P>(
        &self, args: &Vec<&str>, envs: I, cwd: Option<&P>, capture: bool
    ) -> Result<String, shell::ShellError> 
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path> {
        let config = self.config.borrow();
        if config.runtime.dryrun {
            let cmd = args.join(" ");
            println!("dryrun: {}", cmd);
            return Ok(cmd);
        } else if config.should_silent_shell_exec() {
            // regardless for the value of `capture`, always capture value
            let mut cmd = self.create_command(args, envs, cwd, true);
            return Native::run_as_child(&mut cmd);
        } else {
            let mut cmd = self.create_command(args, envs, cwd, capture);
            return Native::run_as_child(&mut cmd);
        }
    }
}
impl Native {
    fn create_command<I, K, V, P>(
        &self, args: &Vec<&str>, envs: I, cwd: Option<&P>, capture: bool
    ) -> Command 
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path> {
        let mut c = Command::new(args[0]);
        c.args(&args[1..]);
        c.envs(envs);
        match cwd {
            Some(d) => { c.current_dir(d.as_ref()); },
            None => match &self.cwd {
                Some(cwd) => {
                    c.current_dir(cwd); 
                    log::trace!("create_command:[{}]@[{}]", args.join(" "), cwd);
                },
                _ => {
                    log::trace!("create_command:[{}]", args.join(" "));
                }
            }
        }
        c.envs(&self.envs);
        if capture {
            c.stdout(Stdio::piped());
            c.stderr(Stdio::piped());
        }
        return c;
    }
    fn get_output(cmd: &mut Command) -> Result<String, shell::ShellError> {
        // TODO: option to capture stderr as no error case
        match cmd.output() {
            Ok(output) => {
                if output.status.success() { 
                    match String::from_utf8(output.stdout) {
                        Ok(s) => return Ok(s.trim().to_string()),
                        Err(err) => return Err(shell::ShellError::OtherFailure{
                            cause: format!("stdout character code error {:?}", err),
                            cmd: format!("{:?}", cmd)
                        })
                    }
                } else {
                    match String::from_utf8(output.stderr) {
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
            },
            Err(err) => return Err(shell::ShellError::OtherFailure{
                cause: format!("get output error {:?}", err),
                cmd: format!("{:?}", cmd)
            })
        }
    }
    fn read_stdout_or_empty(stdout: Option<ChildStdout>) -> String {
        let mut buf = String::new();
        match stdout {
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
        return buf;
    }
    fn run_as_child(cmd: &mut Command) -> Result<String,shell::ShellError> {
        match cmd.spawn() {
            Ok(mut process) => {
                match process.wait() { 
                    Ok(status) => {
                        if status.success() {
                            let mut s = String::new();
                            match process.stdout {
                                Some(mut stream) => match stream.read_to_string(&mut s) {
                                    Ok(_) => return Ok(s.trim().to_string()),
                                    Err(err) => return Err(shell::ShellError::OtherFailure{
                                        cause: format!("read stream error {:?}", err),
                                        cmd: format!("{:?}", cmd)
                                    })
                                },
                                None => Ok("".to_string())
                            }
                        } else {
                            let mut s = String::new();
                            let output = match process.stderr {
                                Some(mut stream) => {
                                    match stream.read_to_string(&mut s) {
                                        Ok(_) => if s.is_empty() { Self::read_stdout_or_empty(process.stdout) } else { s },
                                        Err(_) => Self::read_stdout_or_empty(process.stdout)
                                    }
                                },
                                None => Self::read_stdout_or_empty(process.stdout)
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
