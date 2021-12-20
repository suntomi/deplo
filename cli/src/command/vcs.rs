use std::error::Error;

use log;

use core::config;
use core::shell;

use crate::args;
use crate::command;
use crate::util::escalate;

pub struct VCS<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub shell: S
}
impl<S: shell::Shell> VCS<S> {
    fn release<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        return Ok(())
    }
    fn release_assets<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        return Ok(())
    }
}

impl<S: shell::Shell, A: args::Args> command::Command<A> for VCS<S> {
    fn new(config: &config::Container) -> Result<VCS<S>, Box<dyn Error>> {
        return Ok(VCS::<S> {
            config: config.clone(),
            shell: S::new(config)
        });
    }
    fn run(&self, args: &A) -> Result<(), Box<dyn Error>> {
        match args.subcommand() {
            Some(("release", subargs)) => return self.release(&subargs),
            Some(("release-assets", subargs)) => return self.release_assets(&subargs),
            Some((name, _)) => return escalate!(args.error(
                &format!("no such subcommand: [{}]", name) 
            )),
            None => return escalate!(args.error("no subcommand specified"))
        }
    }
}
