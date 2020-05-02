use std::error::Error;

use log;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;

pub struct Gcloud<'a> {
    pub config: &'a config::Config,
    pub shell: shell::Shell<'a>
}

impl<'a> command::Command<'a> for Gcloud<'a> {
    fn new(config: &'a config::Config) -> Result<Gcloud<'a>, Box<dyn Error>> {
        return Ok(Gcloud {
            config: config,
            shell: shell::Shell::new(config)
        });
    }
    fn run(&self, args: &args::Args) -> Result<(), Box<dyn Error>> {
        log::info!("gcloud command invoked");
        match args.subcommand_matches() {
            Some(m) => {
                match m.values_of("args") {
                    Some(it) => {
                        let subcommand_args: Vec<&str> = it.collect();
                        return match self.shell.invoke("gcloud", &subcommand_args) {
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
            cause: "no argument for gcloud".to_string() 
        }))
    }
}
