use std::error::Error;

use log;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;

pub struct Exec<'a> {
    pub config: &'a config::Config,
    pub shell: shell::Shell<'a>
}

impl<'a> command::Command<'a> for Exec<'a> {
    fn new(config: &'a config::Config) -> Result<Exec<'a>, Box<dyn Error>> {
        return Ok(Exec {
            config: config,
            shell: shell::Shell::new(config)
        });
    }
    fn run(&self, args: &args::Args) -> Result<(), Box<dyn Error>> {
        log::info!("exec command invoked");
        match args.subcommand() {
            Some((_, m)) => {
                match m.values_of("args") {
                    Some(it) => {
                        let subcommand_args: Vec<&str> = it.collect();
                        return match self.shell.exec(&subcommand_args) {
                            Ok(_) => Ok(()),
                            Err(err) => Err(err)
                        }
                    },
                    None => {}
                }
            },
            None => {}
        }
        return Err(Box::new(args::ArgsError{ 
            cause: "no argument for exec".to_string() 
        }))
    }
}
