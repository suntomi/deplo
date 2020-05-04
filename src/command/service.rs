use std::error::Error;
use std::fs;

use log;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;

pub struct Service<'a, S: shell::Shell<'a> = shell::Default<'a>> {
    pub config: &'a config::Config<'a>,
    pub shell: S
}

impl<'a, S: shell::Shell<'a>> Service<'a, S> {
    fn create<A: args::Args>(&self, _: &A) -> Result<(), Box<dyn Error>> {
        log::info!("service create invoked");
        return Ok(())
    }
    fn deploy<A: args::Args>(&self, _: &A) -> Result<(), Box<dyn Error>> {
        log::info!("service deploy invoked");      
        return Ok(())
    }
}

impl<'a, S: shell::Shell<'a>, A: args::Args> command::Command<'a, A> for Service<'a, S> {
    fn new(config: &'a config::Config) -> Result<Service<'a, S>, Box<dyn Error>> {
        return Ok(Service::<'a, S> {
            config: config,
            shell: S::new(config)
        });
    }
    fn run(&self, args: &A) -> Result<(), Box<dyn Error>> {
        log::info!("service command invoked");
        match args.subcommand() {
            Some(("create", subargs)) => return self.create(&subargs),
            Some(("deploy", subargs)) => return self.deploy(&subargs),
            Some((name, _)) => return Err(Box::new(command::CommandError {
                cause: format!("deplo service: no such subcommand: [{}]", name) 
            })),
            None => return Err(Box::new(command::CommandError {
                cause: "deplo service: no subcommand specified".to_string() 
            }))
        }
    }
}
