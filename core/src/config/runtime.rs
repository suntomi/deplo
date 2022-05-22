use std::collections::{HashMap};
use std::error::Error;

use maplit::hashmap;
use serde::{Deserialize, Serialize};

use crate::args::{Args};
use crate::config;

/// runtime configuration for single job execution
#[derive(Serialize, Deserialize, Clone)]
pub struct ExecOptions {
    pub envs: HashMap<String, String>,
    pub revision: Option<String>,
    pub verbosity: u64,
    pub remote: bool,
    pub silent: bool,
    pub timeout: Option<u64>,
}
impl ExecOptions {
    pub fn new<A: Args>(args: &A, config: &config::Container) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            envs: args.map_of("env"),
            revision: args.value_of("ref").map(|v| v.to_string()),
            verbosity: args.value_of("verbosity").unwrap_or("0").parse()?,
            remote: args.occurence_of("remote") > 0,
            silent: args.occurence_of("silent") > 0,
            timeout: args.value_of("timeout").map(|v| v.parse().expect(
                &format!("value of `timeout` should be a number but {}", v)
            )),
        })
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Command {
    pub args: Option<Vec<String>>
}
impl Command {
    pub fn new_or_none<A: Args>(args: &A) -> Option<Self> {
        match args.subcommand() {
            Some((name, args)) => match name {
                "sh" => Some(Self {
                    args: args.values_of("command").map(|v| v.iter().map(|vv| vv.to_string()).collect())
                }),
                _ => None
            },
            None => None
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Job {
    pub name: String,
    pub command: Option<Command>
}
impl Job {
    pub fn new_or_none<A: Args>(
        args: &A, config: &config::Container
    ) -> Result<Option<Self>, Box<dyn Error>> {
        match args.value_of("job").map(|v| v.to_string()) {
            Some(name) => Ok(Some(Self {
                name,
                command: Command::new_or_none(args)
            })),
            None => Ok(None)
        }
    }
}

/// runtime configuration for workflow execution commands
/// only `deplo start/stop` shold contains this configuration
#[derive(Serialize, Deserialize, Clone)]
pub struct Workflow {
    /// key of config.workflows
    pub name: String,
    /// context of the workflow run. 
    /// deplo uses the data to checks whether each job should be run with the workflow.
    pub context: HashMap<String, config::AnyValue>,
    /// job name to run. if omitted, run all job that inovked with the workflow run
    pub job: Option<Job>,
    /// common running cofiguration of each jobs invoked by the workflow run
    pub exec: ExecOptions,
}
impl Workflow {
    pub fn new<A: Args>(args: &A, config: &config::Container) -> Result<Self, Box<dyn Error>> {
        let trigger = match args.value_of("workflow") {
            Some(v) => Some(crate::ci::WorkflowTrigger::DirectInput{
                name: v.to_string(),
                context: match args.value_of("workflow_context") {
                    Some(v) => serde_json::from_str(v)?,
                    None => hashmap!{}
                }
            }),
            None => match args.value_of("workflow_event_payload") {
                Some(v) => Some(crate::ci::WorkflowTrigger::EventPayload(v.to_string())),
                None => None
            }
        };
        let matches = {
            let config = config.borrow();
            let ci = config.modules.ci_by_env();
            ci.filter_workflows(trigger)?
        };
        let (workflow_name, context) = if matches.len() == 0 {
            panic!("no workflow matches with trigger");
        } else if matches.len() > 2 {
            log::warn!(
                "multiple workflow matches({})",
                matches.iter().map(|(n,_)| {n.to_string()}).collect::<Vec<String>>().join(",")
            );
            &matches[0]
        } else {
            &matches[0]
        };
        Ok(Self {
            name: workflow_name.to_string(),
            job: Job::new_or_none(args, config)?,
            context: context.clone(),
            exec: ExecOptions::new(args, config)?
        })
    }
    pub fn command(&self) -> config::job::Command {
        match &self.job {
            Some(job) => match &job.command {
                Some(c) => match &c.args {
                    Some(args) => config::job::Command::Adhoc(args.join(" ")),
                    None => config::job::Command::Shell,
                },
                None => config::job::Command::Job
            },
            None => config::job::Command::Job
        }
    }
}
/// runtime configuration for all invocation of deplo cli.
#[derive(Default)]
pub struct Config {
    pub verbosity: u64,
    pub dotenv_path: Option<String>,
    pub config_path: String,
    pub workdir: Option<String>,
}
impl Config {
    pub fn with_args<A: Args>(args: &A) -> Self {
        Config {
            verbosity: match args.value_of("verbosity") {
                Some(o) => o.parse().unwrap_or(0),
                None => 0
            },
            dotenv_path: args.value_of("dotenv").map(|o| o.to_string()),
            config_path: args.value_of("config").unwrap_or("Deplo.toml").to_string(),
            workdir: args.value_of("workdir").map_or_else(|| None, |v| Some(v.to_string())),
        }
    }
    pub fn apply(&self) -> Result<(), Box<dyn Error>> {
        self.load_dotenv()?;
        Self::setup_logger(self.verbosity);
        match self.workdir {
            Some(ref wd) => { 
                std::env::set_current_dir(wd)?; 
            },
            None => ()
        };
        Ok(())
    }
    pub fn config_source<'a>(&'a self) -> config::source::Source<'a> {
        config::source::Source::File(self.config_path.as_str())
    }
    fn load_dotenv(&self) -> Result<(), Box<dyn Error>> {
        match self.dotenv_path {
            Some(ref path) => {
                dotenv::from_path(path)?;
            },
            None => {
                dotenv::dotenv().ok();
            }
        };
        Ok(())
    }
    fn setup_logger(verbosity: u64) {
        // apply verbosity
        match std::env::var("RUST_LOG") {
            Ok(v) => {
                if !v.is_empty() {
                    simple_logger::init_with_env().unwrap();
                    return;
                }
            },
            Err(_) => {},
        };
        simple_logger::init_with_level(match 
            match std::env::var("DEPLO_OVERWRITE_VERBOSITY") {
                Ok(v) => if !v.is_empty() {
                    println!("overwrite log verbosity from {} to {}", verbosity, v);
                    v.parse::<u64>().unwrap_or(verbosity)
                } else {
                    verbosity
                },
                Err(_) => verbosity
            } 
        {
            0 => log::Level::Warn,
            1 => log::Level::Info,
            2 => log::Level::Debug,
            3 => log::Level::Trace,
            _ => log::Level::Trace
        }).unwrap();
    }
}

/*
    fn post_apply<A: args::Args>(
        config: &mut config::Container, args: &A
    ) -> Result<(), Box<dyn Error>> {
        // set release target

        // because vcs_service create object which have reference of `c` ,
        // scope of `vcs` should be narrower than this function,
        // to prevent `assignment of borrowed value` error below.
        let release_target = {
            let immc = config.borrow();
            let vcs = immc.vcs_service()?;
            match args.value_of("release-target") {
                Some(v) => match immc.common.release_targets.get(v) {
                    Some(_) => Some(v.to_string()),
                    None => return escalate!(Box::new(config::ConfigError{ 
                        cause: format!("undefined release target name: {}", v)
                    }))
                },
                None => match std::env::var("DEPLO_OVERWRITE_RELEASE_TARGET") {
                    Ok(v) => if !v.is_empty() { 
                        Some(v)
                    } else {
                        vcs.release_target()
                    },
                    Err(_) => vcs.release_target()
                }
            }
        };
        let env_workflow_type = match std::env::var("DEPLO_OVERWRITE_WORKFLOW") {
            Ok(v) => if !v.is_empty() { 
                Some(v)
            } else {
                None
            },
            Err(_) => None
        };
        let workflow_type = match args.value_of("workflow-type") {
            Some(v) => Some(v),
            None => match env_workflow_type {
                // if DEPLO_OVERWRITE_WORKFLOW is set, behave as deploy workflow.
                Some(v) => Some(v),
                None => {
                    let immc = config.borrow();
                    let vcs = immc.vcs_service()?;
                    let (account_name, _) = immc.ci_config_by_env_or_default();
                    let ci = immc.ci_service(account_name)?;
                    match ci.pr_url_from_env()? {
                        Some(_) => Some("integrate"),
                        None => match vcs.pr_url_from_env()? {
                            Some(_) => Some("integrate"),
                            None => match vcs.release_target() {
                                Some(_) => Some("deploy"),
                                None => None
                            }
                        }
                    }
                }
            }
        };
        {
            let mut mutc = config.borrow_mut();
            mutc.runtime.release_target = release_target;
            mutc.runtime.workflow_type = workflow_type;
        }

        // change commit hash
        match std::env::var("DEPLO_OVERWRITE_COMMIT") {
            Ok(v) => if !v.is_empty() {
                let c = config.borrow();
                for (account, ci) in &c.ci_caches {
                    let prev = ci.overwrite_commit(v.as_str()).unwrap();
                    log::info!("{}: overwrite commit from {} to {}", account, prev, v);
                }
            },
            Err(_) => ()
        };
        Ok(())
    }
*/