use std::error::Error;
use std::fs;

use log;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;
use crate::endpoints;

pub struct Init<'a, S: shell::Shell<'a> = shell::Default<'a>> {
    pub config: &'a config::Config<'a>,
    pub shell: S
}

impl<'a, S: shell::Shell<'a>, A: args::Args> command::Command<'a, A> for Init<'a, S> {
    fn new(config: &'a config::Config) -> Result<Init<'a, S>, Box<dyn Error>> {
        return Ok(Init::<'a, S> {
            config: config,
            shell: S::new(config)
        });
    }
    fn run(&self, _: &A) -> Result<(), Box<dyn Error>> {
        log::info!("init command invoked");
        fs::create_dir_all(&self.config.root_path())?;
        fs::create_dir_all(&self.config.services_path())?;
        fs::create_dir_all(&self.config.endpoints_path(None))?;
        for (k, _) in &self.config.common.release_targets {
            log::info!("create versions file for {}", k);
            let ep = endpoints::Endpoints::new(&format!("{}.{}", k, self.config.root_domain()));
            ep.save(self.config.endpoints_path(Some(k)))?;
        }
        return Ok(())
    }
}
