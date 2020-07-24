use std::process::{Command, Stdio};
use std::collections::HashMap;
use std::error::Error;
use std::io::Read;
use std::path::Path;

use maplit::hashmap;

use crate::config;
use crate::shell;

pub struct Native<'a> {
    pub config: &'a config::Config,
    pub cwd: Option<String>,
    pub envs: HashMap<String, String>,
}
impl<'a> shell::Shell<'a> for Native<'a> {
    fn new(config: &'a config::Config) -> Self {
        return Native {
            config: config,
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
        // why we have to write such redundant and inefficient codes just for setting value to hashmap?
        ent.and_modify(|e| *e = val.clone()).or_insert(val);
        Ok(())
    }
    fn output_of(&self, args: &Vec<&str>, envs: &HashMap<&str, &str>) -> Result<String, shell::ShellError> {
        let mut cmd = self.create_command(args, envs, true);
        return Native::get_output(&mut cmd);
    }
    fn exec(
        &self, args: &Vec<&str>, envs: &HashMap<&str, &str>, capture: bool
    ) -> Result<String, shell::ShellError> {
        if self.config.runtime.dryrun {
            let cmd = args.join(" ");
            println!("dryrun: {}", cmd);
            return Ok(cmd);
        } else {
            let mut cmd = self.create_command(args, envs, capture);
            return Native::run_as_child(&mut cmd);
        }
    }
}
impl <'a> Native<'a> {
    fn create_command(&self, args: &Vec<&str>, envs: &HashMap<&str, &str>, capture: bool) -> Command {
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
