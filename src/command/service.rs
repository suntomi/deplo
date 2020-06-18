use std::error::Error;

use log;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;
use crate::lb;
use crate::plan;

#[derive(PartialEq, Clone)]
pub enum ActionType {
    Deploy,
    PullRequest,
}

pub struct Service<'a, S: shell::Shell<'a> = shell::Default<'a>> {
    pub config: &'a config::Config<'a>,
    pub shell: S
}

impl<'a, S: shell::Shell<'a>> Service<'a, S> {
    fn create<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        log::debug!("service create invoked");
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
    fn action<A: args::Args>(&self, args: &A, kind: Option<ActionType>) -> Result<(), Box<dyn Error>> {
        log::debug!("service deploy invoked");      
        let ci = self.config.ci_service()?;
        let p = plan::Plan::<'a>::load(
            self.config, 
            // both required argument
            args.value_of("name").unwrap()
        )?;
        p.exec::<S>(kind.unwrap_or(match ci.pull_request_url()? {
            Some(_) => ActionType::PullRequest,
            _ => ActionType::Deploy
        }) == ActionType::PullRequest)?;
        match p.ports()? {
            Some(ports) => {
                for (n, _) in &ports {
                    let name = if n.is_empty() { &p.service } else { n };
                    self.config.update_service_endpoint_version(name, &p)?;
                }
            },
            None => {
                self.config.update_service_endpoint_version(&p.service, &p)?;
            }
        }
        Ok(())
    }
    fn cutover<A: args::Args>(&self, _: &A) -> Result<(), Box<dyn Error>> {
        log::debug!("service cutover invoked");      
        lb::deploy(self.config, false)
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
            Some(("deploy", subargs)) => return self.action(&subargs,Some(ActionType::Deploy)),
            Some(("pr", subargs)) => return self.action(&subargs, Some(ActionType::PullRequest)),
            Some(("action", subargs)) => return self.action(&subargs, None),
            Some(("cutover", subargs)) => return self.cutover(&subargs),
            Some((name, _)) => return Err(args.error(
                &format!("no such subcommand: [{}]", name) 
            )),
            None => return Err(args.error("no subcommand specified"))
        }
    }
}
