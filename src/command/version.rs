use std::error::Error;

use log;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;

pub struct Version<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub shell: S
}

impl<S: shell::Shell, A: args::Args> command::Command<A> for Version<S> {
    fn new(config: &config::Container) -> Result<Version<S>, Box<dyn Error>> {
        return Ok(Version::<S> {
            config: config.clone(),
            shell: S::new(config)
        });
    }
    fn run(&self, _: &A) -> Result<(), Box<dyn Error>> {
        log::info!("{}({})", config::DEPLO_VERSION, config::DEPLO_GIT_HASH);
        return Ok(())
    }
}
