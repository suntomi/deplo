use std::error::Error;

use log;

use core::config;
use core::shell;

use crate::args;
use crate::command;
use crate::util::escalate;

pub struct CI<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub shell: S
}
impl<S: shell::Shell> CI<S> {
    fn kick<A: args::Args>(&self, _: &A) -> Result<(), Box<dyn Error>> {
        log::info!("kick command invoked");
        let config = self.config.borrow();
        let (account_name, _) = config.ci_config_by_env();
        let ci = config.ci_service(account_name)?;
        let jobs = match ci.pull_request_url()? {
            Some(_) => &config.ci_workflow().integrate,
            None => &config.ci_workflow().deploy,
        };
        let vcs = config.vcs_service()?;
        for (_, job) in jobs {
            let ps = job.patterns.iter().map(|p| {
                std::env::current_dir().unwrap()
                    .join(p)
                    .to_string_lossy().to_string()
            }).collect::<Vec<String>>();
            if vcs.changed(&ps.iter().map(std::ops::Deref::deref).collect()) {
                self.shell.run_code_or_file(&job.command, shell::no_env())?;
            }
        }
        Ok(())
    }
    fn setenv<A: args::Args>(&self, _: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let (account_name, _) = config.ci_config_by_env();
        let ci = config.ci_service(account_name)?;
        config.parse_dotenv(|k,v| ci.set_secret(k, v))
    }
}

impl<S: shell::Shell, A: args::Args> command::Command<A> for CI<S> {
    fn new(config: &config::Container) -> Result<CI<S>, Box<dyn Error>> {
        return Ok(CI::<S> {
            config: config.clone(),
            shell: S::new(config)
        });
    }
    fn run(&self, args: &A) -> Result<(), Box<dyn Error>> {
        match args.subcommand() {
            Some(("kick", subargs)) => return self.kick(&subargs),
            Some(("setenv", subargs)) => return self.setenv(&subargs),
            Some((name, _)) => return escalate!(args.error(
                &format!("no such subcommand: [{}]", name) 
            )),
            None => return escalate!(args.error("no subcommand specified"))
        }
    }
}
