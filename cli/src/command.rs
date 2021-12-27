use std::error::Error;
use std::fmt;

use core::args;
use core::config;

use crate::util::escalate;

pub trait Command<A: args::Args> {
    fn new(config: &config::Container) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn run(&self, args: &A) -> Result<(), Box<dyn Error>>;
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
pub mod destroy;
pub mod ci;
pub mod info;
pub mod vcs;

// factorys
fn factory_by<'a, S: args::Args, T: Command<S> + 'a>(
    config: &config::Container
) -> Result<Box<dyn Command<S> + 'a>, Box<dyn Error>> {
    let cmd = T::new(config)?;
    return Ok(Box::new(cmd) as Box<dyn Command<S> + 'a>);
}

pub fn factory<'a, S: args::Args>(
    name: &str, config: &config::Container
) -> Result<Option<Box<dyn Command<S> + 'a>>, Box<dyn Error>> {
    let cmd = match name {
        "init" => factory_by::<S, init::Init>(config),
        "info" => factory_by::<S, info::Info>(config),
        "destroy" => factory_by::<S, destroy::Destroy>(config),
        "ci" => factory_by::<S, ci::CI>(config),
        "vcs" => factory_by::<S, vcs::VCS>(config),
        _ => return Err(Box::new(CommandError {
            cause: format!("add factory matching pattern for [{}]", name)
        }))
    };
    return match cmd {
        Ok(cmd) => Ok(Some(cmd)),
        Err(err) => escalate!(err)
    }
}