use std::fs;
use std::fmt;
use std::path;
use std::error::Error;
use std::rc::Rc;
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
use crate::vcs;
use crate::ci;
use crate::shell;
use crate::util::{escalate,envsubst};

pub const DEPLO_GIT_HASH: &'static str = env!("GIT_HASH");
pub const DEPLO_VERSION: &'static str = "0.1.0";

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

#[derive(Serialize, Deserialize, Clone, Copy, Eq, PartialEq)]
pub enum RunnerOS {
    Linux,
    Windows,
    MacOS,
}
impl RunnerOS {
    pub fn from_str(s: &str) -> Result<Self, &'static str> {
        match s {
            "linux" => Ok(Self::Linux),
            "windows" => Ok(Self::Windows),
            "macos" => Ok(Self::MacOS),
            _ => Err("unknown OS"),
        }
    }
}
impl fmt::Display for RunnerOS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Linux{..} => write!(f, "Linux"),
            Self::Windows{..} => write!(f, "Windows"),
            Self::MacOS{..} => write!(f, "MacOS"),
        }
    }
}
#[derive(Serialize, Deserialize)]
pub struct Cache {
    pub keys: Vec<String>,
    pub save_key: Option<String>,
    pub path: String
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Runner {
    Machine {
        os: RunnerOS,
        image: Option<String>,
        class: Option<String>,
    },
    Container {
        image: String,
    }
}
#[derive(Serialize, Deserialize)]
pub struct Job {
    pub account: Option<String>,
    pub patterns: Vec<String>,
    pub runner: Runner,
    pub command: String,
    pub env: Option<HashMap<String, String>>,
    pub workdir: Option<String>,
    pub checkout: Option<HashMap<String, String>>,
    pub caches: Option<Vec<Cache>>,
    pub depends: Option<Vec<String>>,
    pub options: Option<HashMap<String, String>>,
}
impl Job {
    pub fn runner_os(&self) -> RunnerOS {
        match &self.runner {
            Runner::Machine{ os, image:_, class:_ } => *os,
            Runner::Container{ image: _ } => RunnerOS::Linux
        }
    }
    pub fn runs_on_machine(&self) -> bool {
        match &self.runner {
            Runner::Machine{ os:_, image:_, class:_ } => true,
            Runner::Container{ image: _ } => false
        }
    }
    pub fn job_env<'a>(&self, config: &'a Config) -> HashMap<String, String> {
        let ci = config.ci_service_by_job(&self).unwrap();
        let env = ci.job_env();
        return match &self.env {
            Some(v) => {
                let mut h = env.clone();
                h.extend(v.clone());
                h
            },
            None => env
        };
    }
}
#[derive(Serialize, Deserialize)]
pub struct WorkflowConfig {
    // we call workflow as combination of jobs
    pub integrate: HashMap<String, Job>,
    pub deploy: HashMap<String, Job>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CIAccount {
    GhAction {
        account: String,
        key: String,
    },
    CircleCI {
        key: String,
    }
}
impl CIAccount {
    pub fn type_matched(&self, t: &str) -> bool {
        return t == self.type_as_str()
    }
    pub fn type_as_str(&self) -> &'static str {
        match self {
            Self::GhAction{..} => "GhAction",
            Self::CircleCI{..} => "CircleCI"
        }
    } 
}
impl fmt::Display for CIAccount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GhAction{..} => write!(f, "ghaction"),
            Self::CircleCI{..} => write!(f, "circleci"),
        }
    }    
}
#[derive(Serialize, Deserialize)]
pub struct CIConfig {
    pub accounts: HashMap<String, CIAccount>,
    pub workflow: WorkflowConfig,
}
impl CIConfig {
    pub fn workflow<'a>(&'a self) -> &'a WorkflowConfig {
        return &self.workflow;
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum VCSConfig {
    Github {
        email: String,
        account: String,
        key: String
    },
    Gitlab {
        email: String,
        account: String,
        key: String
    }
}
impl fmt::Display for VCSConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Github{..} => write!(f, "github"),
            Self::Gitlab{..} => write!(f, "gitlab"),
        }
    }    
}
#[derive(Serialize, Deserialize)]
pub struct CommonConfig {
    pub project_name: String,
    pub deplo_image: Option<String>,
    pub data_dir: Option<String>,
    pub release_targets: HashMap<String, String>,
}
#[derive(Default)]
pub struct RuntimeConfig {
    pub verbosity: u64,
    pub dryrun: bool,
    pub debug: HashMap<String, String>,
    pub distributions: Vec<String>,
    pub latest_endpoint_versions: HashMap<String, u32>,
    pub endpoint_service_map: HashMap<String, String>,
    pub release_target: Option<String>,
    pub workdir: Option<String>,
    pub dotenv_path: Option<String>
}
#[derive(Serialize, Deserialize)]
pub struct Config {
    #[serde(skip)]
    pub runtime: RuntimeConfig,
    pub common: CommonConfig,
    pub vcs: VCSConfig,
    pub ci: CIConfig,

    // object cache
    #[serde(skip)]
    pub ci_caches: HashMap<String, Box<dyn ci::CI>>,
    // HACK: I don't know the way to have uninitialized box object...
    #[serde(skip)]
    pub vcs_cache: Vec<Box<dyn vcs::VCS>>,
}
pub type Ref<'a>= std::cell::Ref<'a, Config>;
#[derive(Clone)]
pub struct Container {
    ptr: Rc<RefCell<Config>>
}
impl Container { 
    pub fn borrow(&self) -> Ref<'_> {
        self.ptr.borrow()
    }
}

impl Config {
    // static factory methods 
    pub fn load(path: &str) -> Result<Config, Box<dyn Error>> {
        let src = fs::read_to_string(path).unwrap();
        let content = envsubst(&src);
        match toml::from_str(&content) {
            Ok(c) => Ok(c),
            Err(err) => escalate!(Box::new(err))
        }
    }
    pub fn create<A: args::Args>(args: &A) -> Result<Container, Box<dyn Error>> {
        // apply working directory
        let may_workdir = args.value_of("workdir");
        match may_workdir {
            Some(wd) => { std::env::set_current_dir(&wd)?; },
            None => {}
        }
        // apply verbosity
        let verbosity = args.occurence_of("verbosity");
        simple_logger::init_with_level(match verbosity {
            0 => log::Level::Warn,
            1 => log::Level::Info,
            2 => log::Level::Debug,
            3 => log::Level::Trace,
            _ => log::Level::Warn
        }).unwrap();
        // load dotenv
        match args.value_of("dotenv") {
            Some(dotenv_path) => match fs::metadata(dotenv_path) {
                Ok(_) => { dotenv::from_filename(dotenv_path).unwrap(); },
                Err(_) => { dotenv::from_readable(dotenv_path.as_bytes()).unwrap(); },
            },
            None => match dotenv() {
                Ok(path) => log::debug!("using .env file at {}", path.to_string_lossy()),
                Err(err) => match Self::ci_type() {
                    Ok(v) => log::info!("ci type = {}, environment variable is provided by CI system", v),
                    Err(_) => log::warn!("non-ci environment but .env not present or cannot load by error [{:?}], this usually means:\n\
                        1. command will be run with incorrect parameter or\n\
                        2. secrets are directly written in deplo.toml\n\
                        please use $repo/.env to provide secrets, or use -e flag to specify its path", 
                        err)
                }
            },
        };
        // println!("DEPLO_CLOUD_ACCESS_KEY:{}", std::env::var("DEPLO_CLOUD_ACCESS_KEY").unwrap());    
        let c = Container{
            ptr: Rc::new(RefCell::new(
                Config::load(args.value_of("config").unwrap_or("./Deplo.toml")).unwrap()
            ))
        };
        // setup runtime configuration (except release_target)
        {
            let mut mutc = c.ptr.borrow_mut();
            mutc.runtime = RuntimeConfig {
                verbosity,
                distributions: vec!(),
                latest_endpoint_versions: hashmap!{},
                endpoint_service_map: hashmap!{},
                dotenv_path: match args.value_of("dotenv") {
                    Some(v) => Some(v.to_string()),
                    None => None
                },
                dryrun: args.occurence_of("dryrun") > 0,
                debug: match args.value_of("debug") {
                    Some(s) => {
                        let mut opts = hashmap!{};
                        for v in s.split(",") {
                            let sp: Vec<&str> = v.split("=").collect();
                            opts.insert(
                                sp[0].to_string(),
                                (if sp.len() > 1 { sp[1] } else { "true" }).to_string()
                            );
                        }
                        opts
                    }
                    None => hashmap!{}
                },
                release_target: None, // set after
                workdir: may_workdir.map(String::from),
            };
        }
        return Ok(c);
    }
    pub fn setup<'a, A: args::Args>(c: &'a mut Container, _: &A) -> Result<&'a Container, Box<dyn Error>> {
        // setup module cache
        Self::setup_ci(&c)?;
        Self::setup_vcs(&c)?;
        // set release target
        {
            // because vcs_service create object which have reference of `c` ,
            // scope of `vcs` should be narrower than this function,
            // to prevent `assignment of borrowed value` error below.
            let release_target = {
                let immc = c.ptr.borrow();
                let vcs = immc.vcs_service()?;
                vcs.release_target()
            };
            let mut mutc = c.ptr.borrow_mut();
            mutc.runtime.release_target = match release_target {
                Some(v) => Some(v),
                None => mutc.get_debug_option("force_set_release_target_to").map_or(
                    None, |v| Some(v.clone())
                )
            }
        }
        return Ok(c);
    }
    pub fn root_path(&self) -> &path::Path {
        return path::Path::new(match self.common.data_dir {
            Some(ref v) => v.as_str(),
            None => "."
        });
    }
    pub fn project_name(&self) -> &str {
        &self.common.project_name
    }
    pub fn deplo_image(&self) -> &str {
        match &self.common.deplo_image {
            Some(v) => v,
            None => "ghcr.io/suntomi/deplo"
        }
    }
    pub fn release_target(&self) -> Option<&str> {
        match &self.runtime.release_target {
            Some(s) => Some(&s),
            None => None
        }
    }
    pub fn ci_cli_options(&self) -> String {
        let wdref = self.runtime.workdir.as_ref();
        return format!("{}", 
            if wdref.is_none() { "".to_string() } else { format!("-w {}", wdref.unwrap()) }
        );
    }
    pub fn parse_dotenv<F>(&self, mut cb: F) -> Result<(), Box<dyn Error>>
    where F: FnMut (&str, &str) -> Result<(), Box<dyn Error>> {
        let dotenv_file_content = match &self.runtime.dotenv_path {
            Some(dotenv_path) => match fs::metadata(dotenv_path) {
                Ok(_) => match fs::read_to_string(dotenv_path) {
                    Ok(content) => content,
                    Err(err) => return escalate!(Box::new(err))
                },
                Err(_) => dotenv_path.to_string(),
            },
            None => match dotenv() {
                Ok(dotenv_path) => match fs::read_to_string(dotenv_path) {
                    Ok(content) => content,
                    Err(err) => return escalate!(Box::new(err))
                },
                Err(err) => return escalate!(Box::new(err))
            }
        };
        let r = BufReader::new(dotenv_file_content.as_bytes());
        let re = Regex::new(r#"^([^=]+)=(.+)$"#).unwrap();
        for read_result in r.lines() {
            match read_result {
                Ok(line) => match re.captures(&line) {
                    Some(c) => {
                        cb(
                            c.get(1).map(|m| m.as_str()).unwrap(),
                            c.get(2).map(|m| m.as_str()).unwrap().trim_matches('"')
                        )?;
                    },
                    None => {},
                },
                Err(_) => {}
            }
        }
        return Ok(())
    }
    pub fn ci_type() -> Result<String, ConfigError> {
        match std::env::var("DEPLO_CI_TYPE") {
            Ok(v) => return Ok(v),
            Err(_) => {
                for (key, value) in hashmap!{
                    "CIRCLE_SHA1" => "CircleCI",
                    "GITHUB_ACTION" => "GhAction"
                } {
                    match std::env::var(key) {
                        Ok(_) => return Ok(value.to_string()),
                        Err(_) => continue
                    }
                }
            }
        }
        return Err(ConfigError{ 
            cause: "you don't set CI type and deplo cannot detect it. abort".to_string()
        })
    }
    pub fn ci_config<'a>(&'a self, account_name: &str) -> &'a CIAccount {
        match &self.ci.accounts.get(account_name) {
            Some(c) => c,
            None => panic!("provider corresponding to account {} does not exist", account_name)
        }
    }
    pub fn ci_config_by_env<'b>(&'b self) -> (&'b str, &'b CIAccount) {
        let t = Self::ci_type().unwrap();
        for (account_name, account) in &self.ci.accounts {
            if account.type_matched(&t) { return (account_name, account) }
        }
        panic!("ci_type = {}, but does not have corresponding CI Config", t)
    }
    pub fn ci_service<'a>(&'a self, account_name: &str) -> Result<&'a Box<dyn ci::CI>, Box<dyn Error>> {
        return match self.ci_caches.get(account_name) {
            Some(ci) => Ok(ci),
            None => escalate!(Box::new(ConfigError{ 
                cause: format!("no ci service for {}", account_name) 
            }))
        } 
    }
    pub fn ci_service_by_job<'a, 'b>(&'a self, job: &'b Job) -> Result<&'a Box<dyn ci::CI>, Box<dyn Error>> {
        let account_name = job.account.as_ref().map_or_else(||"default", |v| v.as_str());
        return match self.ci_caches.get(account_name) {
            Some(ci) => Ok(ci),
            None => escalate!(Box::new(ConfigError{ 
                cause: format!("no ci service for {}", account_name) 
            }))
        } 
    }
    pub fn ci_workflow<'a>(&'a self) -> &'a WorkflowConfig {
        return &self.ci.workflow
    }
    // setup_XXX is called any subcommand invocation. just create module objects
    fn setup_ci(c: &Container) -> Result<(), Box<dyn Error>> {
        let mut caches = hashmap!{};
        {
            let immc = c.borrow();
            for (account_name, _) in &immc.ci.accounts {
                caches.insert(account_name.to_string(), ci::factory(c, account_name)?);
            }
        }
        {
            let mut mutc = c.ptr.borrow_mut();
            mutc.ci_caches = caches;
        }
        Ok(())
    }
    fn setup_vcs(c: &Container) -> Result<(), Box<dyn Error>> {
        let vcs = vcs::factory(c)?;
        let mut mutc = c.ptr.borrow_mut();
        mutc.vcs_cache.push(vcs);
        Ok(())
    }
    // prepare_XXX called from deplo init only. first time initialization
    pub fn prepare_ci(c: &Container, reinit: bool) -> Result<(), Box<dyn Error>> {
        let c = c.ptr.borrow();
        let cache = &c.ci_caches;
        for (_, ci) in cache {
            ci.prepare(reinit)?;
        }
        Ok(())
    }
    pub fn prepare_vcs(c: &Container, reinit: bool) -> Result<(), Box<dyn Error>> {
        let c = c.ptr.borrow();
        let cache = &c.vcs_cache;
        if cache.len() <= 0 {
            return escalate!(Box::new(ConfigError{ 
                cause: format!("no vcs service") 
            }))
        }
        cache[0].prepare(reinit)
    }
    pub fn vcs_service<'a>(&'a self) -> Result<&'a Box<dyn vcs::VCS>, Box<dyn Error>> {
        return if self.vcs_cache.len() > 0 {
            Ok(&self.vcs_cache[0])
        } else {
            escalate!(Box::new(ConfigError{ 
                cause: format!("no vcs service") 
            }))
        }
    }
    pub fn has_debug_option(&self, name: &str) -> bool {
        self.runtime.debug.get(name) != None
    }
    pub fn get_debug_option<'a>(&'a self, name: &str) -> Option<&'a String> {
        self.runtime.debug.get(name)
    }
    pub fn should_silent_shell_exec(&self) -> bool {
        return self.runtime.verbosity <= 0;
    }
    pub fn enumerate_jobs<'a>(&'a self) -> HashMap<(&'a str, &'a str), &'a Job> {
        let mut related_jobs: HashMap<(&'a str, &'a str), &'a Job> = hashmap!{};
        for (kind, jobs) in hashmap!{
            "integrate" => &self.ci.workflow.integrate,
            "deploy" => &self.ci.workflow.deploy
        } {
            for (name, job) in jobs {
                match related_jobs.insert((kind, name), job) {
                    None => {},
                    Some(_) => panic!("duplicated job name for {}: {}", kind, name)
                }
            }
        }
        return related_jobs
    }
    pub fn find_job<'a>(&'a self, name: &str) -> Option<&'a Job> {
        let key = match name.find("-").map(|i| name.split_at(i)).map(|(k, v)| (k, v)) {
            Some(v) => v,
            None => return None
        };
        return match self.enumerate_jobs().get(&key) {
            Some(v) => Some(v),
            None => None
        };
    }
    pub fn run_job(&self, shell: &impl shell::Shell, name: &str) -> Result<String, Box<dyn Error>> {
        match self.find_job(name) {
            Some(v) => {
                match v.runner {
                    Runner::Machine{image:_, ref os, class:_} => {
                        let current_os = shell.detect_os()?;
                        if *os == current_os {
                            // run command directly here
                            shell.eval(&v.command, shell::inherit_and(&v.job_env(self)), v.workdir.as_ref(), false)?;
                        } else {
                            log::debug!("runner os is different from current os {} {}", os, current_os);
                            // runner os is not linux and not same as current os. need to run in CI.
                            let ci = self.ci_service_by_job(v)?;
                            return ci.run_job(name);
                        }
                    },
                    Runner::Container{ ref image } => {
                        if std::env::var("CI").is_ok() {
                            // already run inside container `image`, run command directly here
                            shell.eval(&v.command, shell::inherit_and(&v.job_env(self)), v.workdir.as_ref(), false)?;
                        } else {
                            // running on host. run command in container `image` with docker
                            shell.eval_on_container(
                                image, &v.command, shell::inherit_and(&v.job_env(self)), 
                                v.workdir.as_ref(), false
                            )?;
                        }
                    }
                }
            },
            None => return escalate!(Box::new(
                ConfigError{ cause: format!("job {} not found", name) }
            ))
        }
        Ok("".to_string())
    }    
    pub fn is_main_ci(&self, ci_type: &str) -> bool {
        // default always should exist
        return self.ci.accounts.get("default").unwrap().type_matched(ci_type);
    }
}
