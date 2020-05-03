use std::error::Error;
use std::fmt;

use crate::args;
use crate::config;
use crate::command;

#[derive(Debug)]
pub struct CliError {
    pub cause: String
}
impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl Error for CliError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}


pub fn run(args: &args::Args, config: &config::Config) -> Result<(), Box<dyn Error>> {
    match args.subcommand() {
        Some((name, _)) => {
            let cmd = match command::factory(name, config) {
                Ok(cmd) => match cmd {
                    Some(cmd) => cmd,
                    None => return Err(Box::new(CliError{ 
                        cause: format!("no such subcommand {}", name) 
                    })) 
                }
                Err(err) => return Err(err)
            };
            match cmd.run(args) {
                Ok(()) => return Ok(()),
                Err(err) => return Err(err)
            }
        },
        None => return Err(Box::new(CliError{ 
            cause: format!("no command specified") 
        })) 
    };
}
