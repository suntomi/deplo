use std::collections::{HashMap};
use std::error::Error;
use std::io::Write;
use std::thread::sleep;
use std::time::Duration;

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
        match ci.dispatched_remote_job()? {
            Some(job) => {
                ci.mark_job_executed(&job.name)?;
                log::info!("remote job {} dispatched with payload '{}'", job.name, serde_json::to_string(&job)?);
                return Ok(())
            }
            None => {}
        }
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
    fn exec<A: args::Args>(&self, kind: &str, args: &A) -> Result<(), Box<dyn Error>> {
        let job_name = &format!("{}-{}", kind, args.value_of("name").unwrap());
        match self.exec_job(args, job_name)? {
            Some(job_id) => {
                log::info!("remote job {} id={} running", job_name, job_id);
                if args.occurence_of("async") <= 0 {
                    self.wait_job(args, &job_name, &job_id)?;
                } else {
                    //output job_id to use in user script
                    println!("{}", job_id);
                }
            },
            None => {}
        };
        return Ok(())
    }

    // helpers
    fn make_task_command(task: &str, _: Vec<&str>) -> String {
        // TODO: embedding args into task
        task.to_string()
    }
    fn adhoc_envs<A: args::Args>(args: &A) -> HashMap<String, String> {
        let mut envs = HashMap::<String, String>::new();
        match args.values_of("env") {
            Some(es) => for e in es {
                let mut kv = e.split("=");
                let k = kv.next().unwrap();
                let v = kv.next().unwrap_or("");
                envs.insert(k.to_string(), v.to_string());
            },
            None => {}
        };
        envs
    }
    fn exec_job<A: args::Args>(&self, args: &A, job_name: &str) -> Result<Option<String>, Box<dyn Error>> {
        let config = self.config.borrow();
        let remote_job = config.ci_service_by_job_name(job_name)?.dispatched_remote_job()?;
        // if reach here by remote execution, adopt the settings, otherwise using cli options if any.
        let (commit, remote, command, adhoc_envs) = match remote_job {
            // because this process already run in remote environment, remote is always false.
            Some(ref job) => (job.commit.as_ref().map(|s| s.as_str()), false, Some(job.command.clone()), job.envs.clone()),
            None => (args.value_of("ref"), args.occurence_of("remote") > 0, None, Self::adhoc_envs(args))
        };
        match args.subcommand() {
            Some(("sh", subargs)) => {
                log::info!("running shell for job {} at {}",
                    job_name,
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
                            &self.shell, &job_name, &job, config::Command::Shell, &config::JobRunningOptions {
                                commit, remote, shell_settings: shell::interactive(), adhoc_envs
                            }
                        )
                    },
                    Some(task_args) => if task_args[0].starts_with("@") {
                        log::debug!("running shell task [{}] with args [{}]", task_args[0], task_args[1..].join(" "));
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
                        log::debug!("running shell task: result command: [{}]", command);
                        config.run_job(
                            &self.shell, &job_name, &job, config::Command::Adhoc(command), &config::JobRunningOptions {
                                commit, remote, shell_settings: shell::no_capture(), adhoc_envs
                            }
                        )
                    } else {
                        log::debug!("running shell with adhoc command: [{}]", task_args.join(" "));
                        config.run_job_by_name(
                            &self.shell, &job_name, config::Command::Adhoc(task_args.join(" ")), &config::JobRunningOptions {
                                commit, remote, shell_settings: shell::no_capture(), adhoc_envs,
                            }
                        )
                    }
                }
            },
            Some(("wait", subargs)) => Ok(Some(subargs.value_of("job_id").unwrap().to_string())),
            Some((name, _)) => return escalate!(args.error(&format!("no such subcommand: [{}]", name))),
            None => {
                config.run_job_by_name(&self.shell, &job_name, match command {
                    Some(cmd) => config::Command::Adhoc(cmd),
                    None => config::Command::Job
                }, &config::JobRunningOptions {
                    commit, remote, shell_settings: shell::no_capture(), adhoc_envs,
                })
            }
        }
    }
    fn wait_job<A: args::Args>(&self, args: &A, job_name: &str, job_id: &str) -> Result<(), Box<dyn Error>> {
        log::info!("wait for finishing remote job {} id={}", job_name, job_id);
        let config = self.config.borrow();
        let job = match config.find_job(&job_name) {
            Some(job) => job,
            None => return escalate!(args.error(&format!("no such job: [{}]", job_name))),
        };
        let ci = config.ci_service_by_job(&job)?;
        let progress = args.occurence_of("no-progress") == 0;
        let mut timeout = args.value_of("timeout").map(|s| s.parse::<u64>().unwrap_or(0));
        loop {
            match ci.check_job_finished(&job_id)? {
                Some(s) => if progress { 
                    print!(".{}", s);
                    std::io::stdout().flush().unwrap();
                },
                None => {
                    println!(".done");
                    break
                },
            }
            sleep(Duration::from_secs(5));
            match timeout {
                Some(t) => if t > 5 {
                    timeout = Some(t - 5);
                } else {
                    return escalate!(args.error(
                        &format!("remote job {} wait timeout {:?}", 
                        job_name, args.value_of("timeout")
                    )));
                },
                None => {}
            }
        }
        log::info!("remote job {} id={} finished", job_name, job_id);
        Ok(())
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
            Some(("deploy", subargs)) => return self.exec("deploy", &subargs),
            Some(("integrate", subargs)) => return self.exec("integrate", &subargs),
            Some((name, _)) => return escalate!(args.error(
                &format!("no such subcommand: [{}]", name) 
            )),
            None => return escalate!(args.error("no subcommand specified"))
        }
    }
}
