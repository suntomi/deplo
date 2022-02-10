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
    fn make_task_command(task: &str, _: Vec<&str>) -> String {
        // TODO: embedding args into task
        task.to_string()
    }
    fn exec<A: args::Args>(&self, kind: &str, args: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let job_name = &format!("{}-{}", kind, args.value_of("name").unwrap());
        let commit = args.value_of("ref");
        match args.subcommand() {
            Some(("sh", subargs)) => {
                log::info!("running shell for job '{}-{}' at {}",
                    kind, args.value_of("name").unwrap(),
                    commit.unwrap_or("HEAD")
                );
                match subargs.values_of("task") {
                    None => {
                        log::debug!("running interactive shell");
                        let job = match config.find_job(&job_name) {
                            Some(job) => job,
                            None => return escalate!(args.error(&format!("no such job: [{}]", job_name))),
                        };
                        config.run_job(
                            &self.shell, &job_name, &job, &shell::interactive(),
                            config::Command::Shell, commit
                        )?;
                    },
                    Some(task_args) => if task_args[0].starts_with("@") {
                        log::debug!("running shell task '{}' with args '{}'", task_args[0], task_args[1..].join(" "));
                        let job = match config.find_job(&job_name) {
                            Some(job) => job,
                            None => return escalate!(args.error(&format!("no such job: [{}]", job_name))),
                        };
                        let task_name = task_args[0].trim_start_matches("@");
                        let task = match &job.tasks {
                            Some(tasks) => match tasks.get(task_name) {
                                Some(t) => t,
                                None => return escalate!(args.error(&format!("no such task: [{}]", task_name))),
                            }
                            None => return escalate!(args.error(&format!("no tasks definitions: [{}]", task_name))),
                        };
                        let command = Self::make_task_command(&task, task_args[1..].to_vec());
                        log::debug!("running shell task: result command: {}", command);
                        config.run_job(
                            &self.shell, &job_name, &job, &shell::no_capture(),
                            config::Command::Adhoc(command), commit)?;
                    } else {
                        log::debug!("running shell with adhoc command: {}", task_args.join(" "));
                        config.run_job_by_name(
                            &self.shell, &job_name, &shell::no_capture(),
                            config::Command::Adhoc(task_args.join(" ")), commit
                        )?;
                    }
                }
            },
            Some((name, _)) => return escalate!(args.error(&format!("no such subcommand: [{}]", name))),
            None => {
                config.run_job_by_name(&self.shell, &job_name, &shell::no_capture(), config::Command::Job, commit)?;
            }
        }
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
