use std::error::Error;

use log;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;
use crate::lb;
use crate::plan;
use crate::util::escalate;

#[derive(PartialEq, Clone)]
pub enum ActionType {
    Deploy,
    PullRequest,
}

pub struct Service<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub shell: S
}

impl<S: shell::Shell> Service<S> {
    fn create<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        log::debug!("service create invoked");
        let p = plan::Plan::create(
            &self.config, 
            // optoinal
            args.value_of("lb"),
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
        let config = self.config.borrow();        
        let (account_name, _) = config.ci_config_by_env();
        let ci = config.ci_service(account_name)?;
        let plan = plan::Plan::load(
            &self.config, 
            // both required argument
            args.value_of("name").unwrap()
        )?;
        plan.exec::<S>(kind.unwrap_or(match ci.pull_request_url()? {
            Some(_) => ActionType::PullRequest,
            _ => ActionType::Deploy
        }) == ActionType::PullRequest)?;
        match plan.ports()? {
            Some(ports) => {
                for (n, port) in &ports {
                    let name = if n.is_empty() { &plan.service } else { n };
                    let lb_name = port.get_lb_name(&plan);
                    config::Config::update_endpoint_version(&self.config, &lb_name, name, &plan)?;
                }
            },
            None => { // non-container deployment (storage / destribution / etc...)
                config::Config::update_endpoint_version(&self.config, plan.lb_name(), &plan.service, &plan)?;
            }
        }
        Ok(())
    }
    fn cutover<A: args::Args>(&self, _: &A) -> Result<(), Box<dyn Error>> {
        log::debug!("service cutover invoked");      
        lb::deploy(&self.config, false)
    }
}

impl<S: shell::Shell, A: args::Args> command::Command<A> for Service<S> {
    fn new(config: &config::Container) -> Result<Service<S>, Box<dyn Error>> {
        return Ok(Service::<S> {
            config: config.clone(),
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
            Some((name, _)) => return escalate!(args.error(
                &format!("no such subcommand: [{}]", name) 
            )),
            None => return escalate!(args.error("no subcommand specified"))
        }
    }
    fn prerun(&self, _: &A) -> Result<bool, Box<dyn Error>> {
        Ok(false)
    }
}
