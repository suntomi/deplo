use std::error::Error;

use log;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;
use crate::tf;

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
        log::info!("destroy command invoked");
        log::debug!("destroy environment by terraformer");
        let config = self.config.borrow();
        let tf = tf::factory(&self.config)?;
        let c = config.cloud_service("default")?;
        tf.destroy(&c);
        c.cleanup_dependency()?;
        return Ok(())
    }
}
