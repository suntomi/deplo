use std::fs;
use std::fmt;
use std::path::{Path,PathBuf};
use std::error::Error;
use std::rc::Rc;
use std::sync::RwLock;
use std::cell::{RefCell};
use std::collections::{HashMap};
use std::io::{BufReader, BufRead};

use log;
use simple_logger;
use serde::{Deserialize, Serialize};
use dotenv::dotenv;
use maplit::hashmap;
use regex::Regex;

use crate::args;
use crate::util::{
    defer,
    escalate,
    envsubst,
    make_absolute,
    merge_hashmap,
    path_join,
    randombytes_as_string,
    rm
};

pub mod ci;
pub mod job;
pub mod module;
pub mod release_target;
pub mod runtime;
pub mod secret;
pub mod source;
pub mod value;
pub mod vcs;
pub mod workflow;

pub const DEPLO_GIT_HASH: &'static str = env!("GIT_HASH");
pub const DEPLO_VERSION: &'static str = env!("DEPLO_RELEASE_VERSION");
pub const DEPLO_RELEASE_URL_BASE: &'static str = "https://github.com/suntomi/deplo/releases/download";
pub const DEPLO_REMOTE_JOB_EVENT_TYPE: &'static str = "deplo-run-remote-job";
pub const DEPLO_VCS_TEMPORARY_WORKSPACE_NAME: &'static str = "deplo-tmp-workspace";
pub const DEPLO_JOB_OUTPUT_TEMPORARY_FILE: &'static str = "deplo-tmp-job-output.json";
pub const DEPLO_SYSTEM_OUTPUT_COMMIT_BRANCH_NAME: &'static str = "COMMIT_BRANCH";

pub type Value = value::Value;
pub type AnyValue = value::Any;

#[derive(Debug)]
pub struct ConfigError {
    pub cause: String
}
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}
pub type ConfigVersion = u64;
fn default_config_version() -> ConfigVersion { 1 }
#[derive(Default)]
pub struct Modules {
    pub ci: HashMap<String, Box<dyn crate::ci::CI>>,
    // TODO: better way to have uninitialized box variables?
    pub vcs: Option<Box<dyn crate::vcs::VCS>>,
    pub steps: HashMap<String, Box<dyn crate::step::Step>>,
    pub workflows: HashMap<String, Box<dyn crate::workflow::Workflow>>,
}
#[derive(Serialize, Deserialize)]
pub struct Config {
    // config that loads from config file
    #[serde(default = "default_config_version")]
    pub version: ConfigVersion,
    pub project_name: Value,
    pub data_dir: Option<Value>,
    pub debug: Option<HashMap<String, Value>>,
    pub release_targets: HashMap<String, release_target::ReleaseTarget>,
    pub vcs: vcs::Account,
    pub ci: HashMap<String, ci::Account>,
    pub workflows: HashMap<String, workflow::Workflow>,
    pub jobs: HashMap<String, job::Job>,

    // config that get from args
    #[serde(skip)]
    pub runtime: runtime::Config,

    // module object
    #[serde(skip)]
    pub modules: Modules
}
pub type Ref<'a>= std::cell::Ref<'a, Config>;
pub type RefMut<'a> = std::cell::RefMut<'a, Config>;
#[derive(Clone)]
pub struct Container {
    ptr: Rc<RefCell<Config>>
}
impl Container { 
    pub fn borrow(&self) -> Ref<'_> {
        self.ptr.borrow()
    }
    pub fn borrow_mut(&self) -> RefMut<'_> {
        self.ptr.borrow_mut()
    }
    pub fn load_modules(&self) -> Result<(), Box<dyn Error>> {
        // load vcs modules
        let vcs = crate::vcs::factory(self)?;
        // load ci modules
        let mut ci = hashmap!{};
        for (k, _) in &self.borrow().ci {
            ci.insert(k.to_string(), crate::ci::factory(self, k)?);
        }
        // load step modules
        let mut steps = hashmap!{};

        // load workflow modules
        let mut workflows = hashmap!{};

        // store modules
        let mut c = self.borrow_mut();
        c.modules.vcs = Some(vcs);
        c.modules.ci = ci;
        c.modules.steps = steps;
        c.modules.workflows = workflows;
        Ok(())
    }
}

impl Config {
    // static factory methods 
    pub fn containerize(c: Config) -> Container {
        Container{ ptr: Rc::new(RefCell::new(c)) }
    }
    pub fn with(src: Option<&str>) -> Result<Container, Box<dyn Error>> {
        let src = source::Source::Memory(src.unwrap_or(include_str!("../res/test/dummy-Deplo.toml")));
        let mut c = src.load_as::<Config>()?;
        c.runtime = runtime::Config::default();
        return Ok(Self::containerize(c));
    }
    pub fn create<A: args::Args>(args: &A) -> Result<Container, Box<dyn Error>> {
        // generate runtime config
        let runtime_config =  runtime::Config::with_args(args);
        runtime_config.apply()?;
        // config source
        let src = runtime_config.config_source();
        // 1. load secret config and setup
        let secret_config = src.load_as::<secret::Config>()?;
        secret_config.apply()?; // after here, all secrets in Deplo.toml will be resolvable
        // 2. create deplo config
        let c = {
            let mut config = src.load_as::<Config>()?;
            config.runtime = runtime_config;
            Self::containerize(config)
        };
        // 3. load modules
        c.load_modules()?;
        return Ok(c);
    }
}