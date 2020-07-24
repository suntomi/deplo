use std::error::Error;

use log;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;
use crate::tf;

pub struct Destroy<'a, S: shell::Shell<'a> = shell::Default<'a>> {
    pub config: &'a config::Config,
    pub shell: S
}

impl<'a, S: shell::Shell<'a>, A: args::Args> command::Command<'a, A> for Destroy<'a, S> {
    fn new(config: &'a config::Config) -> Result<Destroy<'a, S>, Box<dyn Error>> {
        return Ok(Destroy::<'a, S> {
            config: config,
            shell: S::new(config)
        });
    }
    fn run(&self, _: &A) -> Result<(), Box<dyn Error>> {
        log::info!("destroy command invoked");
        log::debug!("destroy environment by terraformer");
        let tf = tf::factory(self.config)?;
        let c = self.config.cloud_service("default")?;
        tf.destroy(&c);
        c.cleanup_dependency()?;
        return Ok(())
    }
}
