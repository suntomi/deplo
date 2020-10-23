use std::error::Error;

use serde::{Deserialize, Serialize};

use crate::args;
use crate::config;
use crate::command;
use crate::shell;
use crate::util::escalate;

pub struct Info<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub shell: S
}

#[derive(Serialize, Deserialize)]
pub struct Version<'a> {
    pub cli: &'a str,
    pub commit: &'a str,
    pub toolset: &'a str
}

impl<S: shell::Shell> Info<S> {
    fn version<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        let version = Version {
            cli: config::DEPLO_VERSION,
            commit: config::DEPLO_GIT_HASH,
            toolset: config::DEPLO_TOOLSET_HASH
        };
        match args.value_of("output") {
            Some(v) => match v {
                "json" => {
                    println!("{}", serde_json::to_string_pretty(&version)?);
                    return Ok(())
                },
                "plain" => {
                    println!("cli:{}\ncommit:{}\ntoolset:{}", 
                        config::DEPLO_VERSION, config::DEPLO_GIT_HASH, config::DEPLO_TOOLSET_HASH
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
            Some(("version", subargs)) => return self.version(&subargs),
            Some((name, _)) => return escalate!(args.error(
                &format!("no such subcommand: [{}]", name) 
            )),
            None => return escalate!(args.error("no subcommand specified"))
        }
    }
}
