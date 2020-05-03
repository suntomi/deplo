use std::error::Error;

use super::args;
use super::config;

pub trait Command<'a> {
    fn new(config: &'a config::Config) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn run(&self, args: &args::Args) -> Result<(), Box<dyn Error>>;
}

// subcommands
pub mod init;
pub mod exec;

// factory
fn factory_by<'a, T: Command<'a> + 'a>(
    config: &'a config::Config
) -> Result<Box<dyn Command<'a> + 'a>, Box<dyn Error>> {
    let cmd = T::new(config).unwrap();
    return Ok(Box::new(cmd) as Box<dyn Command<'a> + 'a>);
}

pub fn factory<'a>(
    name: &str, config: &'a config::Config
) -> Result<Option<Box<dyn Command<'a> + 'a>>, Box<dyn Error>> {
    let cmd = match name {
        "init" => factory_by::<init::Init>(config),
        "exec" => factory_by::<exec::Exec>(config),
        _ => return Ok(None)
    };
    return match cmd {
        Ok(cmd) => Ok(Some(cmd)),
        Err(err) => Err(err)
    }
}
