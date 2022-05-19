use std::collections::{HashMap};
use std::error::Error;

use crate::args;
use crate::config;

/// runtime configuration for job execution commands
/// only `deplo run` contains this configuration
#[derive(Default)]
pub struct JobConfig {
    pub job_name: String,
    pub workflow: String,
    pub wofkflow_params: HashMap<String, config::AnyValue>,
    pub release_target: Option<String>,
    pub process_envs: HashMap<String, String>,
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
    pub fn with_args<A: args::Args>(args: &A) -> Self {
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
        config: &mut super::Container, args: &A
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
                    None => return escalate!(Box::new(super::ConfigError{ 
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