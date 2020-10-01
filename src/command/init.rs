use std::error::Error;
use std::fs;

use log;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;

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
    fn run(&self, _: &A) -> Result<(), Box<dyn Error>> {
        log::info!("init command invoked");
        let config = self.config.borrow();
        fs::create_dir_all(&config.root_path())?;
        fs::create_dir_all(&config.services_path())?;

        log::info!("create new environment by terraformer");
        let tf = config.terraformer()?;
        tf.init()?;
        tf.exec()?;

        return Ok(())
    }
}
