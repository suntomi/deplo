use std::process::{Command};
use std::fmt;
use std::error::Error;

use crate::config;

pub struct Shell<'a> {
    pub config: &'a config::Config   
}

#[derive(Debug)]
pub struct ShellError {
    cause: String
}

impl fmt::Display for ShellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl Error for ShellError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}



impl<'a> Shell<'a> {
    pub fn new(config: &'a config::Config) -> Self {
        return Shell {
            config: config
        }
    }
    fn create_command(args: &Vec<&str>) -> Command {
        let mut c = Command::new(args[0]);
        for arg in args.iter().skip(1) {
            c.arg(arg);
        }
        return c;
    }
    fn get_output(cmd: &mut Command) -> Result<String, Box<dyn Error>> {
        match cmd.output() {
            Ok(output) => {
                if output.status.success() { 
                    match String::from_utf8(output.stdout) {
                        Ok(s) => return Ok(s),
                        Err(err) => return Err(Box::new(err))
                    }
                } else {
                    match String::from_utf8(output.stdout) {
                        Ok(s) => return Err(Box::new(ShellError{ cause: s })),
                        Err(err) => return Err(Box::new(err))
                    }
                }
            },
            Err(err) => Err(Box::new(err))
        }
    }
    fn run_as_child(cmd: &mut Command) -> Result<(), Box<dyn Error>> {
        match cmd.spawn() {
            Ok(mut process) => {
                match process.wait() { 
                    Ok(status) => {
                        if status.success() {
                            return Ok(());
                        } else {
                            return Err(Box::new(ShellError{ 
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
    #[allow(dead_code)]
    pub fn output_of(&self, args: &Vec<&str>) -> Result<String, Box<dyn Error>> {
        let mut cmd = Shell::<'a>::create_command(args);
        return Shell::get_output(&mut cmd);
    }
    pub fn exec(&self, args: &Vec<&str>) -> Result<(), Box<dyn Error>> {
        if self.config.cli.dryrun {
            let executed = format!("{}", args.join(" "));
            log::info!("dryrun: {}", executed);
            return Ok(());
        } else {
            log::info!("exec: {}", args.join(" "));
            let mut cmd = Shell::<'a>::create_command(args);
            return Shell::run_as_child(&mut cmd);
        }
    }
}
