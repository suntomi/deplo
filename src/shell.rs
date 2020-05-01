use std::process::{Command};
use std::io::Read;
use std::error::Error;

use crate::config;

pub struct Shell<'a> {
    pub config: &'a config::Config   
}

impl<'a> Shell<'a> {
    pub fn new(config: &'a config::Config) -> Self {
        return Shell {
            config: config
        }
    }
    pub fn read_from(&self, args: String) -> Result<String, Box<dyn Error>> {
        let mut cmd = Command::new(args);
        match cmd.spawn() {
            Ok(process) => {
                let mut s = String::new();
                match process.stdout.unwrap().read_to_string(&mut s) {
                    Ok(_) => {},
                    Err(err) => return Err(Box::new(err))
                }
                return Ok(s)
            },
            Err(err) => Err(Box::new(err))
        }
    }
    pub fn invoke(&self, args: String) -> Result<(), Box<dyn Error>> {
        if self.config.cli.dryrun {
            log::info!("dryrun: {}", args);
            return Ok(())
        } else {
            let mut cmd = Command::new(args);
            match cmd.spawn() {
                Ok(_) => Ok(()),
                Err(err) => Err(Box::new(err))
            }
        }
    }
}
