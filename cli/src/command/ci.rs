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
        log::debug!("kick command invoked");
        // init diff data on the fly
        let diff = {
            let config = self.config.borrow();
            let vcs = config.vcs_service()?;
            vcs.make_diff()?
        };
        {
            let mut config_mut = self.config.borrow_mut();
            let vcs_mut = config_mut.vcs_service_mut()?;
            vcs_mut.init_diff(diff)?;
        }
        let config = self.config.borrow();
        let (account_name, _) = config.ci_config_by_env();
        let ci = config.ci_service(account_name)?;
        ci.kick()?;
        let vcs = config.vcs_service()?;
        let jobs_and_kind = match config.runtime.workflow_type {
            Some(v) => match v {
                config::WorkflowType::Integrate => (&config.ci_workflow().integrate, v),
                config::WorkflowType::Deploy => (&config.ci_workflow().deploy, v)
            }
            None => if config.ci.invoke_for_all_branches.unwrap_or(false) {
                (&config.ci_workflow().integrate, config::WorkflowType::Integrate)
            } else if let Some(v) = ci.pr_url_from_env()? {
                panic!("PR url is set by env {}, but workflow type is not set", v)
            } else {
                log::info!("deplo is configured as ignoring non-release-target, non-pull-requested branches");
                return Ok(());
            }
        };
        for (name, job) in jobs_and_kind.0 {
            let full_name = &format!("{}-{}", jobs_and_kind.1.as_str(), name);
            if vcs.changed(&job.patterns.iter().map(|p| p.as_ref()).collect()) &&
                job.matches_current_release_target(&config.runtime.release_target) {
                if config.runtime.dryrun {
                    log::info!("dryrun mode, skip running job {}", full_name);
                    continue;
                }
                log::debug!("========== invoking {}, pattern [{}] ==========", full_name, job.patterns.join(", "));
                ci.mark_job_executed(&full_name)?;
            } else {
                log::debug!("========== not invoking {}, pattern [{}] ==========", full_name, job.patterns.join(", "));
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
    fn fin<A: args::Args>(&self, _: &A) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn exec<A: args::Args>(&self, kind: &str, args: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        config.run_job_by_name(&self.shell, &format!("{}-{}", kind, args.value_of("name").unwrap()))?;
        return Ok(())
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
            Some(("fin", subargs)) => return self.fin(&subargs),
            Some(("deploy", subargs)) => return self.exec("deploy", &subargs),
            Some(("integrate", subargs)) => return self.exec("integrate", &subargs),
            Some((name, _)) => return escalate!(args.error(
                &format!("no such subcommand: [{}]", name) 
            )),
            None => return escalate!(args.error("no subcommand specified"))
        }
    }
}
