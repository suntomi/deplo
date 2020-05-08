use std::error::Error;

use log;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;

mod plan;

pub struct Service<'a, S: shell::Shell<'a> = shell::Default<'a>> {
    pub config: &'a config::Config<'a>,
    pub shell: S
}

impl<'a, S: shell::Shell<'a>> Service<'a, S> {
    fn create<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        log::info!("service create invoked");
        let p = plan::Plan::<'a>::create(
            self.config, 
            // both required argument
            args.value_of("name").unwrap(), 
            args.value_of("type").unwrap()
        )?;
        log::info!("plan created");
        match p.save() {
            Ok(_) => Ok(()),
            Err(err) => {
                println!("save error: {:?}", err);
                Err(err)
            }
        }
    }
    fn deploy<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        log::info!("service deploy invoked");      
        return plan::Plan::<'a>::load(
            self.config, 
            // both required argument
            args.value_of("name").unwrap()
        )?.exec(&self.shell);
    }
}

impl<'a, S: shell::Shell<'a>, A: args::Args> command::Command<'a, A> for Service<'a, S> {
    fn new(config: &'a config::Config) -> Result<Service<'a, S>, Box<dyn Error>> {
        return Ok(Service::<'a, S> {
            config: config,
            shell: S::new(config)
        });
    }
    fn run(&self, args: &A) -> Result<(), Box<dyn Error>> {
        match args.subcommand() {
            Some(("create", subargs)) => return self.create(&subargs),
            Some(("deploy", subargs)) => return self.deploy(&subargs),
            Some((name, _)) => return Err(args.error(
                &format!("no such subcommand: [{}]", name) 
            )),
            None => return Err(args.error("no subcommand specified"))
        }
    }
}
