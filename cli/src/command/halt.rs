use std::error::Error;

use log;

use core::args;
use core::config;
use core::shell;

use crate::command;

pub struct Halt<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    #[allow(dead_code)]
    pub shell: S
}

impl<S: shell::Shell, A: args::Args> command::Command<A> for Halt<S> {
    fn new(config: &config::Container) -> Result<Halt<S>, Box<dyn Error>> {
        return Ok(Halt::<S> {
            config: config.clone(),
            shell: S::new(config)
        });
    }
    fn run(&self, args: &A) -> Result<(), Box<dyn Error>> {
        log::debug!("halt command invoked");
        let workflow = config::runtime::Workflow::new(args, &self.config, false)?;
        let config = self.config.borrow();
        config.jobs.halt(&config, &workflow)?;
        return Ok(())
    }
}
