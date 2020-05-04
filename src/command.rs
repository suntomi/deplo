use std::error::Error;
use std::fmt;

use super::args;
use super::config;

pub trait Command<'a, T: args::Args> {
    fn new(config: &'a config::Config) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn run(&self, args: &T) -> Result<(), Box<dyn Error>>;
}

#[derive(Debug)]
pub struct CommandError {
    cause: String
}
impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl Error for CommandError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

// subcommands
pub mod init;
pub mod exec;
pub mod service;

// factorys
fn factory_by<'a, S: args::Args, T: Command<'a, S> + 'a>(
    config: &'a config::Config
) -> Result<Box<dyn Command<'a, S> + 'a>, Box<dyn Error>> {
    let cmd = T::new(config).unwrap();
    return Ok(Box::new(cmd) as Box<dyn Command<'a, S> + 'a>);
}

pub fn factory<'a, S: args::Args>(
    name: &str, config: &'a config::Config
) -> Result<Option<Box<dyn Command<'a, S> + 'a>>, Box<dyn Error>> {
    let cmd = match name {
        "init" => factory_by::<S, init::Init>(config),
        "exec" => factory_by::<S, exec::Exec>(config),
        _ => return Ok(None)
    };
    return match cmd {
        Ok(cmd) => Ok(Some(cmd)),
        Err(err) => Err(err)
    }
}
