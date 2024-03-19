use std::collections::{HashMap};
use std::error::Error;
use std::fmt::{self};
use std::fs;
use std::path::Path;

use maplit::hashmap;
use serde::{Deserialize, Serialize};

use crate::args::{Args};
use crate::config;
use crate::util::{merge_hashmap};

/// remote execution payload
#[derive(Deserialize)]
pub struct SystemDispatchWorkflowPayload {
    id: String,
    workflow: String,
    context: String,
    exec: String,
    job: String,
    command: Option<String>,
}
// implement std::display::Display for SystemDispatchWorkflowPayload
impl fmt::Display for SystemDispatchWorkflowPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "id:{} workflow:{} context:{} exec:{} job:{}",
            self.id, self.workflow, self.context, self.exec, self.job
        )
    }
}

/// when debugger runs after job is done
#[derive(Serialize, Deserialize, Clone)]
pub enum StartDebugOn {
    #[serde(rename = "default")]
    Default, // see env var DEPLO_CI_START_DEBUG_DEFAULT
    #[serde(rename = "always")]
    Always, // always
    #[serde(rename = "failure")]
    Failure, // on failure
    #[serde(rename = "never")]
    Never, // never
}
impl fmt::Display for StartDebugOn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => write!(f, "default"),
            Self::Always => write!(f, "always"),
            Self::Failure => write!(f, "failure"),
            Self::Never => write!(f, "never"),
        }
    }
}
impl StartDebugOn {
    pub fn should_start(&self, job_failure: bool) -> bool {
        match self {
            Self::Default => std::env::var("DEPLO_CI_START_DEBUG_DEFAULT").map_or(false, |v| {
                match v.as_str() {
                    "always" => true,
                    "failure" => job_failure,
                    "never" => false,
                    "" => false,
                    _ => {
                        log::warn!("DEPLO_CI_START_DEBUG_DEFAULT should be one of 'always', 'failure', 'never', '' but [{}], fallback to behaviour of `failure`", v);
                        job_failure
                    }
                }
            }),
            Self::Always => true,
            Self::Failure => job_failure,
            Self::Never => false
        }
    }
}

/// runtime configuration for single job execution
#[derive(Serialize, Deserialize, Clone)]
pub struct ExecOptions {
    pub envs: HashMap<String, String>,
    pub revision: Option<String>,
    pub release_target: Option<String>,
    pub debug: StartDebugOn,
    pub debug_job: Option<String>,
    pub verbosity: u64,
    pub remote: bool,
    pub follow_dependency: bool,
    pub silent: bool,
    pub timeout: Option<u64>,
}
impl ExecOptions {
    pub fn default() -> Self {
        Self {
            envs: hashmap!{},
            revision: None,
            release_target: None,
            debug: StartDebugOn::Default,
            debug_job: None,
            verbosity: 0,
            remote: false,
            follow_dependency: false,
            silent: false,
            timeout: None,
        }
    }
    pub fn new<A: Args>(args: &A, config: &config::Container, has_job_config: bool) -> Result<Self, Box<dyn Error>> {
        let mut instance = Self::default();
        // rust simple_logger's verbosity cannot be changed once it set, and here already configured.
        // so simply we override verbosity with process' verbosity.
        // on remote running on CI, verbosity should configured with same value as cli specified,
        // via envvar DEPLO_OVERWRITE_EXEC_OPTIONS(JSON)'s verbosity.
        instance.verbosity = config.borrow().runtime.verbosity;
        instance.apply(args, config, has_job_config);
        Ok(instance)
    }
    pub fn with_json(json: &str) -> Self {
        serde_json::from_str(json).expect(&format!("exec options should be valid json but {}", json))
    }
    pub fn apply<A: Args>(&mut self, args: &A, config: &config::Container, has_job_config: bool) {
        // merge parameters from command line args, basically value does not change if cli arg not specified.
        self.envs = merge_hashmap(&self.envs, &args.map_of("env"));
        self.revision = match args.value_of("revision") {
            Some(v) => Some(v.to_string()),
            None => self.revision.clone()
        };
        self.release_target = match args.value_of("release_target") {
            Some(v) => Some(v.to_string()),
            None => match self.release_target {
                Some(ref v) => Some(v.clone()),
                None => {
                    let c = config.borrow();
                    c.modules.vcs().release_target()
                }
            }
        };
        self.debug = match args.value_of("debug") {
            Some(v) => match v {
                "always"|"a" => StartDebugOn::Always,
                "failure"|"f" => StartDebugOn::Failure,
                "never"|"n" => StartDebugOn::Never,
                _ => {
                    log::warn!("value of `debug` should be one of 'always', 'failure', 'never' but {}", v);
                    self.debug.clone()
                }
            },
            None => self.debug.clone()
        };
        self.debug_job = match args.value_of("debug-job") {
            Some(v) => Some(v.to_string()),
            None => self.debug_job.clone()
        };
        self.timeout = match args.value_of("timeout") {
            Some(v) => Some(v.parse().expect(
                &format!("value of `timeout` should be a number but {}", v)
            )),
            None => self.timeout
        };
        // remote/follow_dependency always apply cmdline parameter,
        // to avoid these parameter from event payload wrongly used.
        self.remote = args.occurence_of("remote") > 0;
        self.follow_dependency = if has_job_config {
            args.occurence_of("follow_dependency") > 0
        } else {
            false // deplo boot does not see the option
            // but if the option is set with remote option, each child job run with the option value true.
            // so we set the option false if it does not has job config.
        };
        if args.occurence_of("silent") > 0 {
            self.silent = true;
        }
    }
    pub fn debug_should_start(&self, job: &str, job_failure: bool) -> bool {
        if self.debug.should_start(job_failure) {
            return match &self.debug_job {
                Some(j) => j == job,
                None => match job {
                    // if debug_job is not specified, deplo-boot/deplo-halt is ignored.
                    "deplo-boot"| "deplo-halt" => job_failure,
                    _ => true
                }
            }
        }
        return false;
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Command {
    pub args: Option<Vec<String>>
}
impl Command {
    pub fn new_or_none<A: Args>(args: &A, job: &config::job::Job) -> Option<Self> {
        match args.subcommand() {
            Some((name, args)) => match name {
                "sh" => Some(Self {
                    args: job.command_args(args.values_of("task"))
                }),
                _ => None
            },
            None => None
        }
    }
    pub fn with_vec(args: Vec<String>) -> Self {
        Self { args: Some(args) }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Job {
    pub name: String,
    pub command: Option<Command>
}
impl Job {
    pub fn new<A: Args>(
        args: &A, config: &config::Container
    ) -> Self {
        let name = args.value_of("job").map(|v| v.to_string()).expect("job name should be specified");
        let command = Command::new_or_none(
            args,
            config.borrow().jobs.find(&name).expect(&format!("config for job {} does not exist", name))
        );
        Self { name, command }
    }
    pub fn apply<A: Args>(
        &mut self, args: &A, config: &config::Container
    ) {
        match args.value_of("job").map(|v| v.to_string()) {
            Some(v) => self.name = v,
            None => {},
        }
        match config.borrow().jobs.find(&self.name) {
            Some(j) => match Command::new_or_none(args, j) {
                Some(c) => self.command = Some(c),
                None => {}
            },
            None => {}
        };
    }
}

/// runtime configuration for workflow execution commands
/// `deplo start/stop/run/i/d` shold contains this configuration
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
    pub fn new<A: Args>(args: &A, config: &config::Container, has_job_config: bool) -> Result<Self, Box<dyn Error>> {
        match args.value_of("workflow") {
            // directly specify workflow_name and context
            Some(v) => return Ok(Self {
                name: v.to_string(),
                job: if has_job_config { Some(Job::new(args, config)) } else { None },
                context: match args.value_of("workflow_context") {
                    Some(v) => match fs::read_to_string(Path::new(v)) {
                        Ok(s) => {
                            log::debug!("read context payload from {}", v);
                            serde_json::from_str(&s)?
                        },
                        Err(_) => serde_json::from_str(v)?
                    },
                    None => hashmap!{}
                },
                exec: ExecOptions::new(args, config, has_job_config)?
            }),
            None => {
                let trigger = match args.value_of("workflow_event_payload") {
                    Some(v) => match fs::read_to_string(Path::new(v)) {
                        Ok(s) => {
                            log::debug!("read event payload from {}", v);
                            Some(crate::ci::WorkflowTrigger::EventPayload(s.to_string()))
                        },
                        Err(_) => Some(crate::ci::WorkflowTrigger::EventPayload(v.to_string()))
                    },
                    None => None
                };
                let mut matches = {
                    let config = config.borrow();
                    let (_, ci) = config.modules.ci_by_env();
                    ci.filter_workflows(trigger)?
                };
                if matches.len() == 0 {
                    panic!("no workflow matches with trigger")
                } else if matches.len() > 2 {
                    log::warn!(
                        "multiple workflow matches({})",
                        matches.iter().map(|m| {m.name.as_str()}).collect::<Vec<&str>>().join(",")
                    );
                }
                let mut v = matches.remove(0);
                v.apply(args, config, has_job_config);
                Ok(v)
            }
        }
    }
    pub fn with_context(name: String, context: HashMap<String, config::AnyValue>) -> Self {
        Self { name, context, job: None, exec: ExecOptions::default() }
    }
    pub fn with_system_dispatch(payload: &SystemDispatchWorkflowPayload) -> Self {
        Self {
            name: payload.workflow.to_string(),
            context: serde_json::from_str(&payload.context).expect(
                &format!("context should be valid json but {}", payload.context)
            ), // generate from payload.context
            job: Some(Job{ name: payload.job.clone(), command: payload.command.as_ref().map(|v| {
                Command::with_vec(v.split(" ").map(|v| v.to_string()).collect()) 
            })}),
            exec: ExecOptions::with_json(&payload.exec) // generate with payload.exec,

        }
    }
    pub fn with_payload(payload: &str) -> Result<Self, Box<dyn Error>> {
        Ok(serde_json::from_str(payload)?)
    }
    pub fn apply<A: Args>(&mut self, args: &A, config: &config::Container, has_job_config: bool) {
        self.exec.apply(args, config, has_job_config);
        if has_job_config { 
            match self.job.as_mut() {
                Some(j) => j.apply(args, config),
                None => self.job = Some(Job::new(args, config))
            };
        }
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
    pub debug_options: HashMap<String, String>,
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
            debug_options: args.map_of("debug")
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
    pub fn setup_logger(verbosity: u64) {
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
        match simple_logger::init_with_level(match verbosity {
            0 => log::Level::Warn,
            1 => log::Level::Info,
            2 => log::Level::Debug,
            3 => log::Level::Trace,
            _ => log::Level::Trace
        }) {
            Ok(_) => {},
            Err(e) => panic!("fail to init logger {}", e)
        }
    }
}
