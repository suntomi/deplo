use std::error::Error;

use log;
use maplit::hashmap;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;
use crate::util::escalate;

pub struct Exec<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub shell: S
}

impl<S: shell::Shell, A: args::Args> command::Command<A> for Exec<S> {
    fn new(config: &config::Container) -> Result<Exec<S>, Box<dyn Error>> {
        return Ok(Exec {
            config: config.clone(),
            shell: S::new(config)
        });
    }
    fn run(&self, args: &A) -> Result<(), Box<dyn Error>> {
        log::info!("exec command invoked");
        match args.values_of("args") {
            Some(subargs) => {
                return match self.shell.exec(&subargs, &hashmap!{}, false) {
                    Ok(_) => Ok(()),
                    Err(err) => escalate!(Box::new(err))
                }
            },
            None => escalate!(args.error("no argument for exec"))
        }
    }
}
