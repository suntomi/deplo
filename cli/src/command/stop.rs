use std::error::Error;

use log;

use core::args;
use core::config;
use core::shell;

use crate::command;

pub struct Start<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub shell: S
}

impl<S: shell::Shell, A: args::Args> command::Command<A> for Start<S> {
    fn new(config: &config::Container) -> Result<Start<S>, Box<dyn Error>> {
        return Ok(Start::<S> {
            config: config.clone(),
            shell: S::new(config)
        });
    }
    fn run(&self, args: &A) -> Result<(), Box<dyn Error>> {
        log::debug!("stop command invoked");
        let workflow = config::runtime::Workflow::new(args, &self.config)?;
        let config = self.config.borrow();
        config.jobs.halt(&config, &workflow)?;
        return Ok(())
    }
}
