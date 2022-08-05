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
use crate::util::{make_absolute, escalate, path_join, randombytes_as_string};

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
pub const DEPLO_MODULE_EVENT_TYPE: &'static str = "deplo-send-module-payload";
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
        self.vcs.as_ref().expect("vcs module should be loaded")
    }
    pub fn vcs_mut(&mut self) -> &mut Box<dyn crate::vcs::VCS> {
        self.vcs.as_mut().expect("vcs module should be loaded")
    }
    pub fn ci(&self) -> &HashMap<String, Box<dyn crate::ci::CI>> {
        &self.ci
    }
    pub fn ci_for(&self, account_name: &str) -> &Box<dyn crate::ci::CI> {
        self.ci.get(account_name).expect(&format!("missing ci module for account '{}'", account_name))
    }
    pub fn ci_by_default(&self) -> &Box<dyn crate::ci::CI> {
        self.ci_for("default")
    }
    pub fn ci_by_env(&self) -> (&str, &Box<dyn crate::ci::CI>) {
        for (k, v) in &self.ci {
            if v.runs_on_service() {
                return (k, v);
            }
        }
        return ("default", self.ci_by_default());
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
    pub workflows: workflow::Workflows,
    pub jobs: job::Jobs,

    // config that get from args
    #[serde(skip)]
    pub runtime: runtime::Config,

    #[serde(skip)]
    pub envs: HashMap<String, String>,

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
    pub fn load_vcs_modules(&self) -> Result<(), Box<dyn Error>> {
        let vcs = crate::vcs::factory(self)?;
        {
            let mut c = self.borrow_mut();
            c.modules.vcs = Some(vcs);
        }
        let vcs_envs = {
            let c = self.borrow();
            c.vcs_process_envs()?
        };
        {
            let mut c = self.borrow_mut();
            c.set_process_envs(vcs_envs);
        }
        Ok(())
    }
    pub fn load_ci_modules(&self) -> Result<(), Box<dyn Error>> {
        let mut ci = hashmap!{};
        for (k, _) in self.borrow().ci.as_map() {
            ci.insert(k.to_string(), crate::ci::factory(self, &k)?);
        }
        {
            let mut c = self.borrow_mut();
            c.modules.ci = ci;
        }
        let ci_envs = {
            let c = self.borrow();
            c.ci_process_envs()?
        };
        {
            let mut c = self.borrow_mut();
            c.set_process_envs(ci_envs);
        }
        Ok(())
    }
    pub fn load_modules(&self) -> Result<(), Box<dyn Error>> {
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
        c.modules.steps = steps;
        c.modules.workflows = workflows;
        Ok(())
    }
    pub fn prepare_workflow(&self) -> Result<(), Box<dyn Error>> {
        // vcs: init diff data on the fly
        let diff = {
            let config = self.borrow();
            let vcs = config.modules.vcs();
            vcs.make_diff()?
        };
        {
            let mut config_mut = self.borrow_mut();
            let vcs = config_mut.modules.vcs_mut();
            vcs.init_diff(diff)?;
        };
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
    fn setup(&mut self) {
        self.workflows.setup();
        self.jobs.setup();
    }
    pub fn set_process_envs<K: AsRef<str>,V: AsRef<str>>(&mut self, envs: HashMap<K, Option<V>>) {
        let process_envs = &mut self.envs;
        for (k, v) in &envs {
            match v {
                Some(v) => {
                    std::env::set_var(k.as_ref(), v.as_ref());
                    process_envs.insert(k.as_ref().to_string(), v.as_ref().to_string());
                },
                None => {
                    std::env::remove_var(k.as_ref());
                    process_envs.remove(k.as_ref());
                },
            }
        }
    }
    pub fn vcs_process_envs(&self) -> Result<HashMap<&'static str, Option<&'static str>>, Box<dyn Error>> {
        Ok(hashmap!{
            "DEPLO_CI_CLI_COMMIT_HASH" => Some(DEPLO_GIT_HASH),
            "DEPLO_CI_CLI_VERSION" => Some(DEPLO_VERSION),
        })
    }
    pub fn ci_process_envs(&self) -> Result<HashMap<String, Option<String>>, Box<dyn Error>> {
        let (account_name, ci) = self.modules.ci_by_env();
        let vcs = self.modules.vcs();
        let (ref_type, ref_path) = vcs.current_ref()?;
        let (may_tag, may_branch) = if ref_type == crate::vcs::RefType::Tag {
            (Some(ref_path), None)
        } else {
            (None, Some(ref_path))
        };
        let commit_id = vcs.commit_hash(None)?;
        let random_id = randombytes_as_string!(16);
        let ci_type = self.ci.get(account_name).unwrap().type_as_str().to_string();
        let mut penvs = hashmap!{
            // on local, CI ID should be inherited from parent, if exists.
            // on CI DEPLO_CI_ID replaced with CI specific environment variable that represents canonical ID
            "DEPLO_CI_ID".to_string() => Some(random_id),
            // other CI process env should be calculated, because user may call deplo on non-CI environment.
            // on CI, some of these variables may replaced by CI specific way, by return values of ci.process_env
            "DEPLO_CI_TYPE".to_string() => Some(ci_type),
            "DEPLO_CI_TAG_NAME".to_string() => may_tag,
            "DEPLO_CI_BRANCH_NAME".to_string() => may_branch,
            "DEPLO_CI_CURRENT_COMMIT_ID".to_string() => Some(commit_id),
            // TODO_CI: get pull request url from local execution
            "DEPLO_CI_PULL_REQUEST_URL".to_string() => Some("".to_string())
        };
        let ci_envs = ci.process_env()?;
        for (k, v) in &ci_envs {
            penvs.insert(k.to_string(), Some(v.to_string()));
        }
        Ok(penvs)
    }
    pub fn create<A: args::Args>(args: &A) -> Result<Container, Box<dyn Error>> {
        // generate runtime config
        let runtime_config =  runtime::Config::with_args(args);
        runtime_config.apply()?;
        // config source
        let src = runtime_config.config_source();
        // 1. load secret config and setup
        let secret_config = src.load_as::<secret::Config>()?;
        secret_config.apply_with(&runtime_config)?; // after here, all secrets in Deplo.toml will be resolvable
        // 2. create deplo config
        let c = {
            let mut config = src.load_as::<Config>()?;
            config.runtime = runtime_config;
            config.setup();
            Self::containerize(config)
        };
        // 3. load modules phase 1 (necessary for setup other modules)
        c.load_vcs_modules()?;
        c.load_ci_modules()?;
        // 4. load modules phase 2 (modules not loaded during phase 1)
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
            shell::args![
                "chmod",
                "+x",
                data_path.to_str().expect(&format!("data_path {:?} should be convertable to &str", data_path))
            ], shell::no_env(), shell::no_cwd(), &shell::no_capture()
        )?;
        Ok(())
    }
    pub fn cli_download_url(os: job::RunnerOS, version: &str) -> String {
        return format!("{}/{}/deplo-{}", DEPLO_RELEASE_URL_BASE, version, os.cli_download_postfix());
    }    
    pub fn download_deplo_cli(&self, os: job::RunnerOS, shell: &impl shell::Shell) -> Result<PathBuf, Box<dyn Error>> {
        match std::env::var("DEPLO_DEBUG_CLI_BIN_PATHS") {
            Ok(p) => {
                match serde_json::from_str::<HashMap<String, String>>(&p) {
                    Ok(m) => {
                        if let Some(v) = m.get(&os.to_string()) {
                            let path = make_absolute(v, self.modules.vcs().repository_root()?);
                            log::debug!("deplo_cli_download: use debug cli bin for {}: {}", os, path.to_string_lossy().to_string());
                            return Ok(path);
                        }
                    },
                    Err(e) => {
                        return escalate!(Box::new(ConfigError{ 
                            cause: format!("DEPLO_DEBUG_CLI_BIN_PATHS contains invalid json: {}", e)
                        }))
                    }
                }
            },
            Err(_) => {}
        };
        let base_path = path_join(vec![self.deplo_data_path()?.to_str().unwrap(), "cli", DEPLO_VERSION, os.uname()]);
        let file_path = path_join(vec![base_path.to_str().unwrap(), "deplo"]);
        match fs::metadata(&file_path) {
            Ok(mata) => {
                if mata.is_dir() {
                    return escalate!(Box::new(ConfigError{ 
                        cause: format!("{} exists but not file", file_path.to_string_lossy().to_string())
                    }))
                } else {
                    return Ok(file_path);
                }
            },
            Err(_) => {}
        };
        fs::create_dir_all(&base_path)?;
        shell.download(&Self::cli_download_url(os, DEPLO_VERSION), &file_path.to_str().unwrap(), true)?;
        return Ok(file_path);
    }
    pub fn setup_deplo_cli<S: shell::Shell>(&self, os: job::RunnerOS, shell: &S) -> Result<Option<PathBuf>, Box<dyn Error>> {
        if !Self::is_running_on_ci() {
            Ok(Some(self.download_deplo_cli(os, shell)?))
        } else {
            // on ci, deplo cli should have installed before invoking deplo (if not, how can we invoke deplo itself?)
            Ok(None)
        }
    }
}