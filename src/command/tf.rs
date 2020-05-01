use std::error::Error;

use log;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;

pub struct Terraformer<'a> {
    pub config: &'a config::Config,
    pub shell: shell::Shell<'a>
}

impl<'a> command::Command<'a> for Terraformer<'a> {
    fn new(config: &config::Config) -> Result<Terraformer, Box<dyn Error>> {
        return Ok(Terraformer {
            config: config,
            shell: shell::Shell::new(config)
        });
    }
    fn run(&self, args: &args::Args) -> Result<(), Box<dyn Error>> {
        log::info!("tf command invoked");
        return Ok(());
    }
}
