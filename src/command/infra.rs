use std::error::Error;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;
use crate::util::escalate;

pub struct Infra<'a, S: shell::Shell<'a> = shell::Default<'a>> {
    pub config: &'a config::Config<'a>,
    pub shell: S
}

impl<'a, S: shell::Shell<'a>> Infra<'a, S> {
    fn plan<A: args::Args>(&self, _: &A) -> Result<(), Box<dyn Error>> {
        let tf = self.config.terraformer()?;
        tf.plan()
    }
    fn apply<A: args::Args>(&self, _: &A) -> Result<(), Box<dyn Error>> {
        let tf = self.config.terraformer()?;
        tf.apply()
    }
    fn resource<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        let tf = self.config.terraformer()?;
        match args.value_of("path") {
            Some(path) => print!("{}",tf.eval(&path)?),
            None => print!("{}",tf.rclist()?.join("\n")),
        }
        Ok(())
    }
}

impl<'a, S: shell::Shell<'a>, A: args::Args> command::Command<'a, A> for Infra<'a, S> {
    fn new(config: &'a config::Config) -> Result<Infra<'a, S>, Box<dyn Error>> {
        return Ok(Infra::<'a, S> {
            config: config,
            shell: S::new(config)
        });
    }
    fn run(&self, args: &A) -> Result<(), Box<dyn Error>> {
        // ensure underlying cloud provider initialized
        let _ = self.config.cloud_service()?;
        match args.subcommand() {
            Some(("plan", subargs)) => return self.plan(&subargs),
            Some(("apply", subargs)) => return self.apply(&subargs),
            Some(("rsc", subargs)) => return self.resource(&subargs),
            Some((name, _)) => return escalate!(args.error(
                &format!("no such subcommand: [{}]", name) 
            )),
            None => return escalate!(args.error("no subcommand specified"))
        }
    }
}