use crate::args;
use crate::config;
use crate::command;

pub fn run(args: &args::Args, config: &config::Config) -> Result<(), String> {
    match args.matches.subcommand_name() {
        Some(name) => {
            let cmd = match command::factory(name, config) {
                Ok(cmd) => match cmd {
                    Some(cmd) => cmd,
                    None => return Err(format!("no such subcommand {}", name)) 
                }
                Err(err) => return Err(err.to_string())
            };
            match cmd.run(args) {
                Ok(()) => return Ok(()),
                Err(err) => return Err(err.to_string())
            }
        },
        None => return Err("no subcommand specified".to_string())
    };
}
