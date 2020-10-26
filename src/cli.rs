use std::error::Error;
use std::fmt;

use crate::args;
use crate::config;
use crate::command;
use crate::util::escalate;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub fn version() -> &'static str {
    VERSION
}

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

fn run_common<'a, F, R, A: args::Args>(
    args: &A, config: &config::Container, cb: F
) -> Result<R, Box<dyn Error>>
where F: Fn (&dyn command::Command<A>, &A) -> Result<R, Box<dyn Error>> {
    match args.subcommand() {
        Some((name, subargs)) => {
            let cmd = match command::factory(name, &config) {
                Ok(cmd) => match cmd {
                    Some(cmd) => cmd,
                    None => return Err(Box::new(CliError{ 
                        cause: format!("no such subcommand [{}]", name) 
                    })) 
                }
                Err(err) => return escalate!(err)
            };

            match cb(&*cmd, &subargs) {
                Ok(r) => return Ok(r),
                Err(err) => return escalate!(err)
            }
        },
        None => return Err(Box::new(CliError{ 
            cause: format!("no command specified") 
        })) 
    };
}

pub fn prerun<'a, A: args::Args>(args: &A, config: &config::Container) -> Result<bool, Box<dyn Error>> {
    run_common(args, config, |cmd, subargs| {
        cmd.prerun(subargs)
    })
}
pub fn run<'a, A: args::Args>(args: &A, config: &config::Container) -> Result<(), Box<dyn Error>> {
    run_common(args, config, |cmd, subargs| {
        cmd.run(subargs)
    })
}
