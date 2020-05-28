use std::error::Error;

use log;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;
use crate::lb;

pub mod plan;

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
        let p = plan::Plan::<'a>::load(
            self.config, 
            // both required argument
            args.value_of("name").unwrap()
        )?;
        p.exec::<S>()?;
        match p.ports()? {
            Some(ports) => {
                for (n, _) in &ports {
                    let name = if n.is_empty() { &p.service } else { n };
                    self.config.update_service_endpoint_version(name)?;
                }
            },
            None => {
                self.config.update_service_endpoint_version(&p.service)?;
            }
        }
        Ok(())
    }
    fn cutover<A: args::Args>(&self, _: &A) -> Result<(), Box<dyn Error>> {
        log::info!("service cutover invoked");      
        lb::deploy(self.config)
    }
    fn cleanup<A: args::Args>(&self, _: &A) -> Result<(), Box<dyn Error>> {
        log::info!("service cleanup invoked");      
        lb::cleanup(self.config)
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
            Some(("cutover", subargs)) => return self.cutover(&subargs),
            Some(("cleanup", subargs)) => return self.cleanup(&subargs),
            Some((name, _)) => return Err(args.error(
                &format!("no such subcommand: [{}]", name) 
            )),
            None => return Err(args.error("no subcommand specified"))
        }
    }
}
