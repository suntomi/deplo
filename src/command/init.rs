use std::error::Error;
use std::fs;

use log;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;

pub struct Init<'a, S: shell::Shell<'a> = shell::Default<'a>> {
    pub config: &'a config::Config,
    pub shell: S
}

impl<'a, S: shell::Shell<'a>> command::Command<'a> for Init<'a, S> {
    fn new(config: &'a config::Config) -> Result<Init<'a, S>, Box<dyn Error>> {
        return Ok(Init::<'a, S> {
            config: config,
            shell: S::new(config)
        });
    }
    fn run(&self, _: &args::Args) -> Result<(), Box<dyn Error>> {
        log::info!("init command invoked");
        fs::create_dir_all(&self.config.common.data_dir)?;
        return Ok(())
    }
}
