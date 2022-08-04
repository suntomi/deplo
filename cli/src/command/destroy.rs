use std::error::Error;

use log;

use core::args;
use core::config;
use core::shell;

use crate::command;

pub struct Destroy<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub shell: S
}

impl<S: shell::Shell, A: args::Args> command::Command<A> for Destroy<S> {
    fn new(config: &config::Container) -> Result<Destroy<S>, Box<dyn Error>> {
        return Ok(Destroy::<S> {
            config: config.clone(),
            shell: S::new(config)
        });
    }
    fn run(&self, _: &A) -> Result<(), Box<dyn Error>> {
        log::debug!("destroy command invoked");
        // TODO:
        // remove CI settings and cache directory of deplo cli/module
        return Ok(())
    }
}