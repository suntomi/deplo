use std::error::Error;
use std::fs;

use core::config;
use core::shell;
use core::ci;

use crate::args;
use crate::command;
use crate::util::escalate;

pub struct CI<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub shell: S
}
impl<S: shell::Shell> CI<S> {
    fn setenv<A: args::Args>(&self, _: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let ci = config.modules.ci_by_default();
        for (k,v) in config::secret::vars()? {
            println!("set secret {}", k);
            ci.set_secret(&k, &v)?;
        }
        Ok(())
    }
    fn token<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let (_, ci) = config.modules.ci_by_env();
        match args.subcommand() {
            Some(("oidc", subargs)) => {
                let output = match subargs.value_of("output") {
                    Some(v) => v,
                    None => return escalate!(args.error("ci token oidc: must specify output"))
                };
                match ci.generate_token(&ci::TokenConfig::OIDC{
                    audience: match subargs.value_of("audience") {
                        Some(v) => v.to_string(),
                        None => return escalate!(args.error("ci token oidc: must specify audience"))
                    }
                }) {
                    Ok(token) => {
                        fs::write(output, &token)?;
                        println!("wrote token to {}", output);
                    },
                    Err(e) => return escalate!(args.error(
                        &format!("failed to generate oidc token: {}", e)
                    )),
                }
                Ok(())
            },
            Some((name, _)) => return escalate!(args.error(
                &format!("ci token: no such subcommand: [{}]", name) 
            )),
            None => return escalate!(args.error("ci token: no subcommand specified"))
        }
    }
    fn restore_cache<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let (_, ci) = config.modules.ci_by_env();
        ci.restore_cache(args.occurence_of("submodules") > 0)
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
            Some(("setenv", subargs)) => return self.setenv(&subargs),
            Some(("token", subargs)) => return self.token(&subargs),
            Some(("restore-cache", subargs)) => return self.restore_cache(&subargs),
            Some((name, _)) => return escalate!(args.error(
                &format!("no such subcommand: [{}]", name) 
            )),
            None => return escalate!(args.error("no subcommand specified"))
        }
    }
}
