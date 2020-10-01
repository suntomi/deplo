use std::process::{Command, Stdio};
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
    fn output_of<I, K, V>(
        &self, args: &Vec<&str>, envs: I
    ) -> Result<String, shell::ShellError> 
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr> {
        let mut cmd = self.create_command(args, envs, true);
        return Native::get_output(&mut cmd);
    }
    fn exec<I, K, V>(
        &self, args: &Vec<&str>, envs: I, capture: bool
    ) -> Result<String, shell::ShellError> 
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>{
        if self.config.borrow().runtime.dryrun {
            let cmd = args.join(" ");
            println!("dryrun: {}", cmd);
            return Ok(cmd);
        } else {
            let mut cmd = self.create_command(args, envs, capture);
            return Native::run_as_child(&mut cmd);
        }
    }
}
impl Native {
    fn create_command<I, K, V>(
        &self, args: &Vec<&str>, envs: I, capture: bool
    ) -> Command 
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr> {
        let mut c = Command::new(args[0]);
        c.args(&args[1..]);
        c.envs(envs);
        match &self.cwd {
            Some(cwd) => { 
                c.current_dir(cwd); 
                log::trace!("create_command:[{}]@[{}]", args.join(" "), cwd);
            },
            _ => {
                log::trace!("create_command:[{}]", args.join(" "));
            }
        };
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
                            cause: format!("stdout character code error {:?}", err)
                        })
                    }
                } else {
                    match String::from_utf8(output.stderr) {
                        Ok(s) => return Err(shell::ShellError::OtherFailure{ 
                            cause: format!("command returns error {}", s)
                        }),
                        Err(err) => return Err(shell::ShellError::OtherFailure{
                            cause: format!("stderr character code error {:?}", err)
                        })
                    }
                }
            },
            Err(err) => return Err(shell::ShellError::OtherFailure{
                cause: format!("get output error {:?}", err)
            })
        }
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
                                        cause: format!("read stream error {:?}", err)
                                    })
                                },
                                None => Ok("".to_string())
                            }
                        } else {
                            return match status.code() {
                                Some(_) => Err(shell::ShellError::ExitStatus{ status }),
                                None => Err(shell::ShellError::OtherFailure{
                                    cause: format!("cmd terminated by signal")
                                }),
                            }
                        }
                    },
                    Err(err) => Err(shell::ShellError::OtherFailure{
                        cause: format!("wait process error {:?}", err)
                    })
                }
            },
            Err(err) => Err(shell::ShellError::OtherFailure{
                cause: format!("process spawn error {:?}", err)
            })
        }
    }
}
