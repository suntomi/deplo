use std::process::{Command, Stdio};
use std::collections::HashMap;
use std::error::Error;
use std::io::Read;
use std::path::Path;

use maplit::hashmap;

use crate::config;
use crate::shell;

pub struct Native<'a> {
    pub config: &'a config::Config<'a>,
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
    #[allow(dead_code)]
    fn output_of(&self, args: &Vec<&str>, envs: &HashMap<String, String>) -> Result<String, Box<dyn Error>> {
        let mut cmd = self.create_command(args, envs, true);
        return Native::get_output(&mut cmd);
    }
    fn exec(
        &self, args: &Vec<&str>, envs: &HashMap<String, String>, capture: bool
    ) -> Result<String, Box<dyn Error>> {
        if self.config.runtime.dryrun {
            let cmd = args.join(" ");
            println!("dryrun: {}", cmd);
            return Ok(cmd);
        } else {
            log::trace!("exec: [{}]", args.join(" "));
            let mut cmd = self.create_command(args, envs, capture);
            return Native::run_as_child(&mut cmd);
        }
    }
}
impl <'a> Native<'a> {
    fn create_command(&self, args: &Vec<&str>, envs: &HashMap<String, String>, capture: bool) -> Command {
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
    fn get_output(cmd: &mut Command) -> Result<String, Box<dyn Error>> {
        // TODO: option to capture stderr as no error case
        match cmd.output() {
            Ok(output) => {
                if output.status.success() { 
                    match String::from_utf8(output.stdout) {
                        Ok(s) => return Ok(s),
                        Err(err) => return Err(Box::new(err))
                    }
                } else {
                    match String::from_utf8(output.stderr) {
                        Ok(s) => return Err(Box::new(shell::ShellError{ cause: s })),
                        Err(err) => return Err(Box::new(err))
                    }
                }
            },
            Err(err) => Err(Box::new(err))
        }
    }
    fn run_as_child(cmd: &mut Command) -> Result<String, Box<dyn Error>> {
        match cmd.spawn() {
            Ok(mut process) => {
                match process.wait() { 
                    Ok(status) => {
                        if status.success() {
                            let mut s = String::new();
                            match process.stdout {
                                Some(mut stream) => match stream.read_to_string(& mut s) {
                                    Ok(_) => return Ok(s),
                                    Err(err) => return Err(Box::new(err))
                                },
                                None => Ok("".to_string())
                            }
                        } else {
                            return Err(Box::new(shell::ShellError{ 
                                cause: match status.code() {
                                    Some(code) => format!(
                                        "`{}` exit with {}", format!("{:?}", cmd), code
                                    ),
                                    None => format!("cmd terminated by signal")
                                }
                            }));
                        }
                    },
                    Err(err) => Err(Box::new(err))
                }
            },
            Err(err) => Err(Box::new(err))
        }
    }
}
