use std::collections::{HashMap};
use std::error::Error;
use std::io::Write;
use std::thread::sleep;
use std::time::Duration;

use log;
use maplit::hashmap;

use core::config;
use core::shell;
use core::vcs;

use crate::args;
use crate::command;
use crate::util::{escalate};

struct AggregatedPullRequestOptions {
    labels: Vec<String>,
    assignees: Vec<String>,
}
pub struct CI<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub shell: S
}
impl<S: shell::Shell> CI<S> {    
    fn kick<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        // log::debug!("kick command invoked");
        // // init diff data on the fly
        // let diff = {
        //     let config = self.config.borrow();
        //     let vcs = config.vcs_service()?;
        //     vcs.make_diff()?
        // };
        // {
        //     let mut config_mut = self.config.borrow_mut();
        //     let vcs_mut = config_mut.vcs_service_mut()?;
        //     vcs_mut.init_diff(diff)?;
        // }
        // let config = self.config.borrow();
        // let (account_name, _) = config.ci_config_by_env_or_default();
        // let ci = config.ci_service(account_name)?;
        // ci.kick()?;
        // match ci.dispatched_remote_job()? {
        //     Some(job) => {
        //         ci.mark_job_executed(&job.name)?;
        //         log::info!("remote job {} dispatched with payload '{}'", job.name, serde_json::to_string(&job)?);
        //         return Ok(())
        //     }
        //     None => {}
        // }
        // let vcs = config.vcs_service()?;
        // let jobs_and_kind = match config.runtime.workflow_type {
        //     Some(v) => match v {
        //         config::WorkflowType::Integrate => (&config.jobs.integrate, v),
        //         config::WorkflowType::Deploy => (&config.jobs.deploy, v)
        //     }
        //     None => if config.ci.invoke_for_all_branches.unwrap_or(false) {
        //         (&config.jobs.integrate, config::WorkflowType::Integrate)
        //     } else if let Some(v) = ci.pr_url_from_env()? {
        //         panic!("PR url is set by env {}, but workflow type is not set", v)
        //     } else {
        //         log::info!("deplo is configured as ignoring non-release-target, non-pull-requested branches");
        //         return Ok(());
        //     }
        // };
        // for (name, job) in jobs_and_kind.0 {
        //     let full_name = &format!("{}-{}", jobs_and_kind.1.as_str(), name);
        //     if vcs.changed(&job.diff_matcher()) &&
        //         job.matches_current_release_target(&config.runtime.release_target) {
        //         if config.runtime.dryrun {
        //             log::info!("dryrun mode, skip running job {}", full_name);
        //             continue;
        //         }
        //         log::debug!("========== invoking {}, pattern [{}] ==========", full_name, job.patterns.join(", "));
        //         match ci.mark_job_executed(&full_name)? {
        //             Some(job_id) => self.wait_job(args, full_name, &job_id)?,
        //             None => {}
        //         }
        //     } else {
        //         log::debug!("========== not invoking {}, pattern [{}] ==========", full_name, job.patterns.join(", "));
        //     }
        // }
        // if !config::Config::is_running_on_ci() {
        //     log::debug!("if not running on CI, all jobs should be finished");
        //     self.cleanup_jobs()?;
        // }
        Ok(())
    }
    fn setenv<A: args::Args>(&self, _: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let ci = config.modules.default_ci();
        for (k,v) in config::secret::vars()? {
            println!("set secret {}", k);
            ci.set_secret(&k, &v)?;
        }
        Ok(())
    }
    fn set_output<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let key = args.value_of("key").unwrap();
        let value = args.value_of("value").unwrap();
        config.jobs.set_user_output(&config, key, value)?;
        Ok(())
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
            None => if !config::Config::is_running_on_ci() {
                log::debug!("the job {} should finish because no job_id. do cleanup", job_name);
                self.cleanup_jobs()?;
            }
        };
        return Ok(())
    }
    fn fin<A: args::Args>(&self, _: &A) -> Result<(), Box<dyn Error>> {
        self.cleanup_jobs()
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
        Ok(None)
        // let config = self.config.borrow();
        // let remote_job = config.ci_service_by_job_name(job_name)?.dispatched_remote_job()?;
        // // if reach here by remote execution, adopt the settings, otherwise using cli options if any.
        // let (commit, remote, command, adhoc_envs) = match remote_job {
        //     // because this process already run in remote environment, remote is always false.
        //     Some(ref rj) => (rj.commit.as_ref().map(|s| s.as_str()), false, rj.command.clone(), rj.envs.clone()),
        //     None => (args.value_of("ref"), args.occurence_of("remote") > 0, None, Self::adhoc_envs(args))
        // };
        // let non_interactive_shell_opts = if args.occurence_of("silent") > 0 {
        //     shell::silent()
        // } else {
        //     shell::no_capture()
        // };
        // let job = match config.find_job(&job_name) {
        //     Some(job) => job,
        //     None => return escalate!(args.error(&format!("no such job: [{}]", job_name))),
        // };
        // match args.subcommand() {
        //     Some(("sh", subargs)) => {
        //         log::info!("running shell for job {} at {}",
        //             job_name,
        //             commit.unwrap_or("HEAD")
        //         );
        //         match subargs.values_of("task") {
        //             None => {
        //                 log::debug!("running interactive shell");
        //                 config.run_job(
        //                     &self.shell, &job_name, &job, config::Command::Shell, &config::JobRunningOptions {
        //                         commit, remote, shell_settings: shell::interactive(), adhoc_envs
        //                     }
        //                 )
        //             },
        //             Some(task_args) => if task_args[0].starts_with("@") {
        //                 log::debug!("running shell task [{}] with args [{}]", task_args[0], task_args[1..].join(" "));
        //                 let task_name = task_args[0].trim_start_matches("@");
        //                 let task = match &job.tasks {
        //                     Some(tasks) => match tasks.get(task_name) {
        //                         Some(t) => t,
        //                         None => return escalate!(args.error(&format!("no such task: [{}]", task_name))),
        //                     }
        //                     None => return escalate!(args.error(&format!("no tasks definitions: [{}]", task_name))),
        //                 };
        //                 let command = Self::make_task_command(&task, task_args[1..].to_vec());
        //                 log::debug!("running shell task: result command: [{}]", command);
        //                 config.run_job(
        //                     &self.shell, &job_name, &job, config::Command::Adhoc(command), &config::JobRunningOptions {
        //                         commit, remote, shell_settings: non_interactive_shell_opts, adhoc_envs
        //                     }
        //                 )
        //             } else {
        //                 log::debug!("running shell with adhoc command: [{}]", task_args.join(" "));
        //                 config.run_job_by_name(
        //                     &self.shell, &job_name, config::Command::Adhoc(task_args.join(" ")), &config::JobRunningOptions {
        //                         commit, remote, shell_settings: non_interactive_shell_opts, adhoc_envs,
        //                     }
        //                 )
        //             }
        //         }
        //     },
        //     Some(("steps", _)) => {
        //         config.run_job_steps(&self.shell, &non_interactive_shell_opts, &job)?;
        //         Ok(None)
        //     },
        //     Some(("wait", subargs)) => Ok(Some(subargs.value_of("job_id").unwrap().to_string())),
        //     Some(("output", subargs)) => {
        //         let key = subargs.value_of("key").unwrap();
        //         match config.user_job_output(job_name, key)? {
        //             Some(v) => println!("{}", v),
        //             None => {}
        //         };
        //         Ok(None)
        //     },
        //     Some((name, _)) => return escalate!(args.error(&format!("no such subcommand: [{}]", name))),
        //     None => {
        //         config.run_job_by_name(&self.shell, &job_name, match command {
        //             Some(cmd) => config::Command::Adhoc(cmd),
        //             None => config::Command::Job
        //         }, &config::JobRunningOptions {
        //             commit, remote, shell_settings: non_interactive_shell_opts, adhoc_envs,
        //         })
        //     }
        // }
    }
    fn wait_job<A: args::Args>(&self, args: &A, job_name: &str, job_id: &str) -> Result<(), Box<dyn Error>> {
        log::info!("wait for finishing remote job {} id={}", job_name, job_id);
        // let config = self.config.borrow();
        // let job = match config.find_job(&job_name) {
        //     Some(job) => job,
        //     None => return escalate!(args.error(&format!("no such job: [{}]", job_name))),
        // };
        // let ci = config.ci_service_by_job(&job)?;
        // let progress = args.occurence_of("no-progress") == 0;
        // let mut timeout = args.value_of("timeout").map(|s| s.parse::<u64>().unwrap_or(0));
        // loop {
        //     match ci.check_job_finished(&job_id)? {
        //         Some(s) => if progress { 
        //             print!(".{}", s);
        //             std::io::stdout().flush().unwrap();
        //         },
        //         None => {
        //             println!(".done");
        //             break
        //         },
        //     }
        //     sleep(Duration::from_secs(5));
        //     match timeout {
        //         Some(t) => if t > 5 {
        //             timeout = Some(t - 5);
        //         } else {
        //             return escalate!(args.error(
        //                 &format!("remote job {} wait timeout {:?}", 
        //                 job_name, args.value_of("timeout")
        //             )));
        //         },
        //         None => {}
        //     }
        // }
        // log::info!("remote job {} id={} finished", job_name, job_id);
        Ok(())
    }
    fn aggregate_commits(&self) -> Result<Vec<(String, Vec<String>, config::job::CommitMethod)>, Box<dyn Error>> {
        let config = self.config.borrow();
        let commits: Vec<(String, Vec<String>, config::job::CommitMethod)> = vec![];
        // let mut aggregated_push_branches = vec![];
        // let mut aggregated_pr_branches = vec![];
        // let mut aggregated_pr_opts = AggregatedPullRequestOptions {
        //     labels: vec![], assignees: vec![],
        // };
        // for (name, job) in config.enumerate_jobs() {
        //     let job_name = format!("{}-{}", name.0, name.1);
        //     match config.system_job_output(&job_name, config::DEPLO_SYSTEM_OUTPUT_COMMIT_BRANCH_NAME)? {
        //         Some(v) => {
        //             log::info!("ci fin: find commit from job {} at {} for target {}", 
        //                 job_name, v, config.runtime.release_target.as_ref().map_or_else(|| "none", |v| v.as_str()));
        //             match job.commit_setting_from_release_target(&config.runtime.release_target).unwrap().push_opts {
        //                 Some(ref options) => match options {
        //                     config::PushOptions::Push{squash} => {
        //                         if squash.unwrap_or(true) {
        //                             aggregated_push_branches.push(v);
        //                         } else {
        //                             commits.push((format!("{}-push", job_name), vec![v], config::PushOptions::Push{squash: Some(false)}));
        //                         }
        //                     },
        //                     config::PushOptions::PullRequest{labels, assignees, aggregate} => {
        //                         if aggregate.unwrap_or(false) {
        //                             aggregated_pr_branches.push(v);
        //                             aggregated_pr_opts.labels = [aggregated_pr_opts.labels, labels.clone().unwrap_or(vec![])].concat();
        //                             aggregated_pr_opts.assignees = [aggregated_pr_opts.assignees, assignees.clone().unwrap_or(vec![])].concat();
        //                         } else {
        //                             commits.push((format!("{}-pr", job_name), vec![v], config::PushOptions::PullRequest{
        //                                 labels: labels.as_ref().map(|v| v.clone()), assignees: assignees.as_ref().map(|v| v.clone()),
        //                                 aggregate: Some(false)
        //                             }));
        //                         }
        //                     }
        //                 },
        //                 // if push option is not set, default behaviour is:
        //                 // push to current branch for integrate jobs,
        //                 // make pr to branch for deploy jobs.
        //                 None => if name.0 == "integrate" {
        //                     // aggregated by default
        //                     aggregated_push_branches.push(v);
        //                 } else {
        //                     // made single PR by default
        //                     commits.push((format!("{}-pr", job_name), vec![v], config::PushOptions::PullRequest{
        //                         labels: None, assignees: None, aggregate: Some(false)
        //                     }));
        //                 }
        //             }
        //         },
        //         None => {}
        //     };
        // }
        // commits.push(("aggregate-push".to_string(), aggregated_push_branches, config::PushOptions::Push{squash: Some(true)}));
        // commits.push(("aggregate-pr".to_string(), aggregated_pr_branches, config::PushOptions::PullRequest{
        //     labels: if aggregated_pr_opts.labels.len() > 0 { Some(aggregated_pr_opts.labels) } else { None }, 
        //     assignees: if aggregated_pr_opts.assignees.len() > 0 { Some(aggregated_pr_opts.assignees) } else { None }, 
        //     aggregate: Some(true)
        // }));
        Ok(commits)
    }
    fn push_job_result_branches(
        &self, branches_and_options: &(String, Vec<String>, config::job::CommitMethod)
    ) -> Result<(), Box<dyn Error>> {
        // let config = self.config.borrow();
        // let vcs = config.vcs_service()?;
        // let job_id = std::env::var("DEPLO_CI_ID").unwrap();
        // let current_branch = std::env::var("DEPLO_CI_BRANCH_NAME").unwrap();
        // let current_ref = std::env::var("DEPLO_CI_CURRENT_SHA").unwrap();
        // let (name, branches, options) = branches_and_options;
        // if branches.len() > 0 {
        //     let working_branch = format!("deplo-auto-commits-{}-{}", job_id, name);
        //     vcs.checkout(&current_ref, Some(&working_branch))?;
        //     for b in branches {
        //         vcs.fetch_branch(&b)?;
        //         // FETCH_HEAD of branch b will be picked
        //         vcs.pick_ref("FETCH_HEAD")?;
        //     }
        //     let result_head_ref = match options {
        //         config::PushOptions::PullRequest{labels,assignees,..} => {
        //             vcs.push_branch(&working_branch, &working_branch, &hashmap!{
        //                 "new" => "true",
        //             })?;
        //             let mut pr_opts_for_vcs = hashmap!{};
        //             match labels {
        //                 Some(v) => { pr_opts_for_vcs.insert("labels", serde_json::to_string(&v)?); },
        //                 None => {}
        //             };
        //             match assignees {
        //                 Some(v) => { pr_opts_for_vcs.insert("assignees", serde_json::to_string(&v)?); },
        //                 None => {}
        //             };
        //             vcs.pr(
        //                 &format!("[deplo] auto commit by job [{}]", job_id),
        //                 &working_branch, &current_branch, 
        //                 &pr_opts_for_vcs.iter().map(|(k,v)| (*k,v.as_str())).collect()
        //             )?;
        //             // if pushed successfully, back to original branch
        //             &current_ref
        //         },
        //         config::PushOptions::Push{squash} => {
        //             if squash.unwrap_or(true) && branches.len() > 1 {
        //                 vcs.squash_branch(branches.len())?;
        //             }
        //             vcs.push_branch(&working_branch, &current_branch, &hashmap!{})?;
        //             // if pushed successfully, move current branch HEAD to pushed HEAD
        //             &working_branch
        //         }
        //     };
        //     // only local execution need to recover repository status
        //     if !config::Config::is_running_on_ci() {
        //         vcs.checkout(result_head_ref, Some(&current_branch))?;
        //         vcs.delete_branch(vcs::RefType::Branch, &working_branch)?;
        //         for b in branches {
        //             vcs.delete_branch(vcs::RefType::Remote, b)?;
        //         }
        //     }
        // }
        Ok(())
    }
    fn cleanup_jobs(&self) -> Result<(), Box<dyn Error>> {
        for branches_and_options in self.aggregate_commits()? {
            match self.push_job_result_branches(&branches_and_options) {
                Ok(_) => {},
                Err(e) => {
                    log::error!("push_job_result_branches fails: back to original branch");
                    let config = self.config.borrow();
                    let vcs = config.modules.vcs();
                    let current_branch = std::env::var("DEPLO_CI_BRANCH_NAME").unwrap();
                    let current_ref = std::env::var("DEPLO_CI_CURRENT_SHA").unwrap();
                    vcs.checkout(&current_ref, Some(&current_branch)).unwrap();
                    return Err(e);
                }
            }
        }
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
            Some(("set-output", subargs)) => return self.set_output(&subargs),
            Some(("deploy", subargs)) => return self.exec("deploy", &subargs),
            Some(("integrate", subargs)) => return self.exec("integrate", &subargs),
            Some(("fin", subargs)) => return self.fin(&subargs),
            Some((name, _)) => return escalate!(args.error(
                &format!("no such subcommand: [{}]", name) 
            )),
            None => return escalate!(args.error("no subcommand specified"))
        }
    }
}
