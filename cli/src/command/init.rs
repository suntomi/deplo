use std::error::Error;
use std::fs;

use log;

use core::args;
use core::config;
use core::shell;

use crate::command;

pub struct Init<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub shell: S
}

impl<S: shell::Shell, A: args::Args> command::Command<A> for Init<S> {
    fn new(config: &config::Container) -> Result<Init<S>, Box<dyn Error>> {
        return Ok(Init::<S> {
            config: config.clone(),
            shell: S::new(config)
        });
    }
    fn run(&self, args: &A) -> Result<(), Box<dyn Error>> {
        log::debug!("init command invoked");
        {
            // use block to release ownership of config before prepare_XXX call
            let config = self.config.borrow();
            fs::create_dir_all(&config.root_path())?;
        }
        // do preparation
        let reinit = args.value_of("reinit").unwrap_or("none");
        config::Config::prepare_ci(&self.config, reinit == "all" || reinit == "ci")?;
        config::Config::prepare_vcs(&self.config, reinit == "all" || reinit == "vcs")?;

        return Ok(())
    }
}
