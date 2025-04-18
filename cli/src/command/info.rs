use std::error::Error;

use serde::{Deserialize, Serialize};

use core::args;
use core::config;
use core::shell;

use crate::command;
use crate::util::escalate;

pub struct Info<S: shell::Shell = shell::Default> {
    #[allow(dead_code)]
    pub config: config::Container,
    #[allow(dead_code)]
    pub shell: S
}

#[derive(Serialize, Deserialize)]
pub struct Version<'a> {
    pub cli: &'a str,
    pub commit: &'a str
}

impl<S: shell::Shell> Info<S> {
    fn version<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        let version = Version {
            cli: config::DEPLO_VERSION,
            commit: config::DEPLO_GIT_HASH
        };
        match args.value_of("output") {
            Some(v) => match v {
                "json" => {
                    println!("{}", serde_json::to_string_pretty(&version)?);
                    return Ok(())
                },
                "plain" => {
                    println!("cli:{}\ncommit:{}", 
                        config::DEPLO_VERSION, config::DEPLO_GIT_HASH
                    );
                    return Ok(())
                },
                _ => {}
            },
            None => {}
        };
        println!("{}", config::DEPLO_VERSION);
        Ok(())
    }
}

impl<S: shell::Shell, A: args::Args> command::Command<A> for Info<S> {
    fn new(config: &config::Container) -> Result<Info<S>, Box<dyn Error>> {
        return Ok(Info::<S> {
            config: config.clone(),
            shell: S::new(config)
        });
    }
    fn run(&self, args: &A) -> Result<(), Box<dyn Error>> {
        match args.subcommand() {
            Some(("version", subargs)) => {
                self.version(&subargs)?;
            },
            Some((name, _)) => return escalate!(args.error(
                &format!("no such subcommand: [{}]", name) 
            )),
            None => return escalate!(args.error("no subcommand specified"))
        }
        Ok(())
    }
}