use std::error::Error;

use super::args;
use super::config;

pub trait Command<'a, T: args::Args> {
    fn new(config: &'a config::Config) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn run(&self, args: &T) -> Result<(), Box<dyn Error>>;
}

// subcommands
pub mod init;
pub mod exec;

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
