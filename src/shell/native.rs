use std::process::{Command};
use std::error::Error;

use crate::config;
use crate::shell;

pub struct Native<'a> {
    pub config: &'a config::Config<'a>   
}
impl<'a> shell::Shell<'a> for Native<'a> {
    fn new(config: &'a config::Config) -> Self {
        return Native {
            config: config
        }
    }
    #[allow(dead_code)]
    fn output_of(&self, args: &Vec<&str>) -> Result<String, Box<dyn Error>> {
        let mut cmd = Native::<'a>::create_command(args);
        return Native::get_output(&mut cmd);
    }
    fn exec(&self, args: &Vec<&str>) -> Result<(), Box<dyn Error>> {
        if self.config.cli.dryrun {
            println!("dryrun: {}", args.join(" "));
            return Ok(());
        } else {
            log::info!("exec: {}", args.join(" "));
            let mut cmd = Native::<'a>::create_command(args);
            return Native::run_as_child(&mut cmd);
        }
    }
}
impl <'a> Native<'a> {
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
                        Ok(s) => return Err(Box::new(shell::ShellError{ cause: s })),
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
