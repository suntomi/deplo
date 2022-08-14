use std::error::Error;

use log;

use core::args;
use core::config;
use core::shell;

use crate::command;

pub struct Boot<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub shell: S
}

impl<S: shell::Shell, A: args::Args> command::Command<A> for Boot<S> {
    fn new(config: &config::Container) -> Result<Boot<S>, Box<dyn Error>> {
        return Ok(Boot::<S> {
            config: config.clone(),
            shell: S::new(config)
        });
    }
    fn run(&self, args: &A) -> Result<(), Box<dyn Error>> {
        log::debug!("boot command invoked");
        self.config.prepare_workflow()?;
        let workflow = config::runtime::Workflow::new(args, &self.config, false)?;
        let config = self.config.borrow();
        config.jobs.boot(&config, &workflow, &self.shell)?;
        return Ok(())
    }
}
