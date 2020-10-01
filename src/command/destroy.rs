use std::error::Error;

use log;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;
use crate::lb;

pub struct Destroy<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub shell: S
}

impl<S: shell::Shell, A: args::Args> command::Command<A> for Destroy<S> {
    fn new(config: &config::Container) -> Result<Destroy<S>, Box<dyn Error>> {
        return Ok(Destroy::<S> {
            config: config.clone(),
            shell: S::new(config)
        });
    }
    fn run(&self, _: &A) -> Result<(), Box<dyn Error>> {
        log::debug!("destroy command invoked");
        log::info!("destroy environment by terraformer");
        lb::cleanup(&self.config)?;
        let config = self.config.borrow();
        let tf = config.terraformer()?;
        tf.destroy()?;
        return Ok(())
    }
}
