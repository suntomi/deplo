use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{PathBuf};
use std::rc::Rc;
use std::cell::{RefCell};
use std::collections::{HashMap};

use serde::{Deserialize, Serialize};
use maplit::hashmap;

use crate::args;
use crate::shell;
use crate::util::{make_absolute, escalate};

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
#[derive(Default)]
pub struct Modules {
    ci: HashMap<String, Box<dyn crate::ci::CI>>,
    // TODO: better way to have uninitialized box variables?
    vcs: Option<Box<dyn crate::vcs::VCS>>,
    steps: HashMap<String, Box<dyn crate::step::Step>>,
    workflows: HashMap<String, Box<dyn crate::workflow::Workflow>>,
}
impl Modules {
    pub fn vcs(&self) -> &Box<dyn crate::vcs::VCS> {
        self.vcs.as_ref().unwrap()
    }
    pub fn ci(&self) -> &HashMap<String, Box<dyn crate::ci::CI>> {
        &self.ci
    }
    pub fn default_ci(&self) -> &Box<dyn crate::ci::CI> {
        self.ci_for("default")
    }
    pub fn ci_for(&self, account_name: &str) -> &Box<dyn crate::ci::CI> {
        self.ci.get(account_name).unwrap()
    }
}
pub type ConfigVersion = u64;
fn default_config_version() -> ConfigVersion { 1 }
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
    pub ci: ci::Accounts,
    pub workflows: HashMap<String, workflow::Workflow>,
    pub jobs: job::Jobs,

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
        for (k, _) in self.borrow().ci.as_map() {
            ci.insert(k.to_string(), crate::ci::factory(self, &k)?);
        }
        // load step modules
        let mut steps = hashmap!{};
        module::config_for::<crate::step::Module, _, (), Box<dyn Error>>(|configs| {
            for c in configs {
                steps.insert(c.uses.resolve().to_string(), crate::step::factory(self, &c.uses, &c.with)?);
            }
            Ok(())
        })?;
        // load workflow modules
        let mut workflows = hashmap!{};
        module::config_for::<crate::workflow::Module, _, (), Box<dyn Error>>(|configs| {
            for c in configs {
                workflows.insert(c.uses.resolve().to_string(), crate::workflow::factory(self, &c.uses, &c.with)?);
            }
            Ok(())
        })?;
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
    pub fn is_running_on_ci() -> bool {
        match std::env::var("CI") {
            Ok(v) => !v.is_empty(),
            Err(_) => false
        }
    }
    pub fn data_dir(&self) -> String {
        return match self.data_dir {
            Some(ref v) => v.resolve(),
            None => ".deplo".to_string()
        };
    }
    pub fn project_name(&self) -> String {
        self.project_name.resolve()
    }
    pub fn deplo_data_path(&self) -> Result<PathBuf, Box<dyn Error>> {
        let base = self.data_dir();
        let path = make_absolute(base, self.modules.vcs().repository_root()?);
        match fs::metadata(&path) {
            Ok(mata) => {
                if !mata.is_dir() {
                    return escalate!(Box::new(ConfigError{ 
                        cause: format!("{} exists but not directory", path.to_string_lossy().to_string())
                    }))
                }
            },
            Err(_) => {
                fs::create_dir_all(&path)?;
            }
        };
        Ok(path)
    }
    pub fn generate_wrapper_script<S: shell::Shell>(
        &self, shell: &S, data_path: &PathBuf
    ) -> Result<(), Box<dyn Error>> {
        let content = format!(
            include_str!("../res/cli/deplow.sh.tmpl"),
            version = DEPLO_VERSION,
            data_dir = self.data_dir()
        );
        fs::write(data_path, content)?;
        shell.exec(
            shell::args!["chmod", "+x", data_path.to_str().unwrap()], shell::no_env(), shell::no_cwd(), &shell::no_capture()
        )?;
        Ok(())
    }
}