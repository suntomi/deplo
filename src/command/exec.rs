use std::error::Error;

use log;
use maplit::hashmap;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;

pub struct Exec<'a, S: shell::Shell<'a> = shell::Default<'a>> {
    pub config: &'a config::Config<'a>,
    pub shell: S
}

impl<'a, S: shell::Shell<'a>, A: args::Args> command::Command<'a, A> for Exec<'a, S> {
    fn new(config: &'a config::Config) -> Result<Exec<'a, S>, Box<dyn Error>> {
        return Ok(Exec {
            config: config,
            shell: S::new(config)
        });
    }
    fn run(&self, args: &A) -> Result<(), Box<dyn Error>> {
        log::info!("exec command invoked");
        match args.values_of("args") {
            Some(subargs) => {
                return match self.shell.exec(&subargs, &hashmap!{}) {
                    Ok(_) => Ok(()),
                    Err(err) => Err(err)
                }
            },
            None => Err(args.error("no argument for exec"))
        }
    }
}
