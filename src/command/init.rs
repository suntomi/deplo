use std::error::Error;

use log;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;

pub struct Init<'a> {
    pub config: &'a config::Config,
    pub shell: shell::Shell<'a>
}

impl<'a> command::Command<'a> for Init<'a> {
    fn new(config: &config::Config) -> Result<Init, Box<dyn Error>> {
        return Ok(Init {
            config: config,
            shell: shell::Shell::new(config)
        });
    }
    fn run(&self, _args: &args::Args) -> Result<(), Box<dyn Error>> {
        log::info!("init command invoked");
        return Ok(())
    }
}
