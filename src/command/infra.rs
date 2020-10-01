use std::error::Error;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;
use crate::util::escalate;

pub struct Infra<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub shell: S
}

impl<S: shell::Shell> Infra<S> {
    fn plan<A: args::Args>(&self, _: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let tf = config.terraformer()?;
        tf.plan()
    }
    fn apply<A: args::Args>(&self, _: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let tf = config.terraformer()?;
        tf.apply()
    }
    fn rm<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let tf = config.terraformer()?;
        match args.value_of("address") {
            Some(path) => return tf.rm(&path),
            None => return escalate!(args.error(
                &format!("resource address is not specified") 
            ))
        }
    }
    fn resource<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let tf = config.terraformer()?;
        match args.value_of("path") {
            Some(path) => print!("{}",tf.eval(&path)?),
            None => print!("{}",tf.rclist()?.join("\n")),
        }
        Ok(())
    }
}

impl<'a, S: shell::Shell, A: args::Args> command::Command<A> for Infra<S> {
    fn new(config: &config::Container) -> Result<Infra<S>, Box<dyn Error>> {
        return Ok(Infra::<S> {
            config: config.clone(),
            shell: S::new(config)
        });
    }
    fn run(&self, args: &A) -> Result<(), Box<dyn Error>> {
        match args.subcommand() {
            Some(("plan", subargs)) => return self.plan(&subargs),
            Some(("apply", subargs)) => return self.apply(&subargs),
            Some(("rm", subargs)) => return self.rm(&subargs),
            Some(("rsc", subargs)) => return self.resource(&subargs),
            Some((name, _)) => return escalate!(args.error(
                &format!("no such subcommand: [{}]", name) 
            )),
            None => return escalate!(args.error("no subcommand specified"))
        }
    }
}