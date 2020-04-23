use maplit;

use crate::args;
use crate::config;
use crate::command;

pub fn run(args: &args::Args, config: &config::Config) -> Result<(), String> {
    let command_map = maplit::hashmap!{
        "gcloud" => command::gcloud::Gcloud::new,
    };    
    match args.matches.subcommand_name() {
        Some(name) => {
            match command_map.get(name) {
                Some(factory) => {
                    let ctor: Box<dyn Fn () -> Result<Box<dyn command::Command>, Box<dyn std::error::Error>>> = 
                    Box::new(|| {
                        let cmd = factory(config)?;
                        return Ok(Box::new(cmd) as Box<dyn command::Command>);
                    });
                    match ctor() {
                        Ok(cmd) => {
                            match cmd.run(args) {
                                Ok(()) => return Ok(()),
                                Err(err) => return Err(err.to_string())
                            }
                        },
                        Err(err) => return Err(err.to_string())
                    }
                },
                None => return Err(format!("no such subcommand {}", name))
            }
        },
        None => return Err("no subcommand specified".to_string())
    };
}
