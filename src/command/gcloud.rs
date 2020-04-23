use std::error::Error;

use log;

use crate::args;
use crate::config;
use crate::command;

pub struct Gcloud<'a> {
    pub config: &'a config::Config
}

impl<'a> Gcloud<'a> {
    pub fn new(config: &'a config::Config) -> Result<Gcloud, Box<dyn Error>> {
        return Ok(Gcloud {
            config: config
        });
    }
}
impl<'a> command::Command for Gcloud<'a> {
    fn run(&self, args: &args::Args) -> Result<(), Box<dyn Error>> {
        log::info!("gcloud command invoked");
        return Ok(());
    }
}
