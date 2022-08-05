use std::error::Error;

use log;

use core::args;
use core::config;
use core::shell;

use crate::command;

pub struct Run<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub shell: S
}

impl<S: shell::Shell, A: args::Args> command::Command<A> for Run<S> {
    fn new(config: &config::Container) -> Result<Run<S>, Box<dyn Error>> {
        return Ok(Run::<S> {
            config: config.clone(),
            shell: S::new(config)
        });
    }
    fn run(&self, args: &A) -> Result<(), Box<dyn Error>> {
        log::debug!("run command invoked");
        self.config.prepare_workflow()?;
        let workflow = config::runtime::Workflow::new(args, &self.config)?;
        let config = self.config.borrow();
        config.jobs.boot(&config, &workflow, &self.shell)?;
        return Ok(());
    }
}