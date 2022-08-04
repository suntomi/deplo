use std::fs;
use std::fmt;
use std::path::{Path,PathBuf};
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
use toml::value::Value as TomlValue;

use crate::args;
use crate::vcs;
use crate::ci;
use crate::shell;
use crate::util::{
    defer,
    escalate,envsubst,
    make_absolute,
    merge_hashmap,
    path_join,
    randombytes_as_string,
    rm
};

pub const DEPLO_GIT_HASH: &'static str = env!("GIT_HASH");
pub const DEPLO_VERSION: &'static str = env!("DEPLO_RELEASE_VERSION");
pub const DEPLO_RELEASE_URL_BASE: &'static str = "https://github.com/suntomi/deplo/releases/download";
pub const DEPLO_REMOTE_JOB_EVENT_TYPE: &'static str = "deplo-run-remote-job";
pub const DEPLO_VCS_TEMPORARY_WORKSPACE_NAME: &'static str = "deplo-tmp-workspace";
pub const DEPLO_JOB_OUTPUT_TEMPORARY_FILE: &'static str = "deplo-tmp-job-output.json";
pub const DEPLO_SYSTEM_OUTPUT_COMMIT_BRANCH_NAME: &'static str = "COMMIT_BRANCH";

pub fn cli_download_url(os: RunnerOS, version: &str) -> String {
    return format!("{}/{}/deplo-{}", DEPLO_RELEASE_URL_BASE, version, os.cli_download_postfix());
}

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
    pub fn uname(&self) -> &'static str {
        match self {
            Self::Linux => "Linux",
            Self::Windows => "Windows",
            Self::MacOS => "Darwin",
        }
    }
    pub fn cli_download_postfix(&self) -> &'static str {
        match self {
            Self::Linux => "Linux",
            Self::Windows => "Windows.exe",
            Self::MacOS => "Darwin",
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
    pub paths: Vec<String>
}
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum FallbackContainer {
    ImageUrl{ image: String, shell: Option<String> },
    DockerFile{ path: String, repo_name: Option<String>, shell: Option<String> },
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Runner {
    Machine {
        os: RunnerOS,
        image: Option<String>,
        class: Option<String>,
        local_fallback: Option<FallbackContainer>,
    },
    Container {
        image: String,
    }
}
#[derive(Eq, PartialEq)]
pub enum Command {
    Adhoc(String),
    Job,
    Shell
}
impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Adhoc(s) => write!(f, "Adhoc({})", s),
            Command::Job => write!(f, "Job"),
            Command::Shell => write!(f, "Shell"),
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PushOptions {
    Push {
        squash: Option<bool>,
    },
    PullRequest {
        labels: Option<Vec<String>>,
        assignees: Option<Vec<String>>,
        aggregate: Option<bool>,
    }
}
#[derive(Serialize, Deserialize)]
pub struct Commit {
    pub patterns: Vec<String>,
    pub for_targets: Option<Vec<String>>,
    pub log_format: Option<String>,
    pub push_opts: Option<PushOptions>,
}
impl Commit {
    fn generate_commit_log(&self, name: &str, _: &Job) -> String {
        self.log_format.as_ref().unwrap_or(
            &format!("[deplo] update by job {job_name}", job_name = name)
        ).to_string()
    }
}
pub struct JobRunningOptions<'a> {
    pub remote: bool,
    pub adhoc_envs: HashMap<String, String>,
    pub shell_settings: shell::Settings,
    pub commit: Option<&'a str>,
}
pub struct SystemJobEnvOptions {
    pub paths: Option<Vec<String>>,
    pub job_name: String,
}
#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Step {
    Command {
        command: String,
        name: Option<String>,
        env: Option<HashMap<String, String>>,
        shell: Option<String>,
        workdir: Option<String>,
    },
    Module {
        uses: String,
        name: Option<String>,
        with: Option<HashMap<String, TomlValue>>
    }
}
impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Step::Command{
                command, env, shell, workdir, name
            } => write!(f, 
                "Step::Command{{name:{}, cmd:{}, env:{}, shell:{}, workdir:{}}}", 
                name.as_ref().map_or_else(|| "".to_string(), |s| s.to_string()),
                command, 
                env.as_ref().map_or_else(|| "None".to_string(), |e| format!("{:?}", e)),
                shell.as_ref().map_or_else(|| "None".to_string(), |e| format!("{:?}", e)),
                workdir.as_ref().map_or_else(|| "None".to_string(), |e| format!("{:?}", e)),
            ),
            Step::Module{
                uses, with, name
            } => write!(f, 
                "Step::Module{{name:{}, uses:{}, with:{}", 
                name.as_ref().map_or_else(|| "".to_string(), |s| s.to_string()),
                uses, with.as_ref().map_or_else(|| "None".to_string(), |e| format!("{:?}", e))
            )
        }
    }
}
pub struct StepsDumper<'a> {
    steps: &'a Vec<Step>,
}
impl<'a> fmt::Display for StepsDumper<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}]", self.steps.iter().map(|v| format!("{}", v)).collect::<Vec<_>>().join(","))
    }
}
#[derive(Serialize, Deserialize)]
pub struct Job {
    pub account: Option<String>,
    pub for_targets: Option<Vec<String>>,
    pub patterns: Vec<String>,
    pub runner: Runner,
    pub shell: Option<String>,
    pub command: Option<String>,
    pub steps: Option<Vec<Step>>,
    pub env: Option<HashMap<String, String>>,
    pub workdir: Option<String>,
    pub checkout: Option<HashMap<String, String>>,
    pub caches: Option<HashMap<String, Cache>>,
    pub depends: Option<Vec<String>>,
    pub commits: Option<Vec<Commit>>,
    pub options: Option<HashMap<String, String>>,
    pub tasks: Option<HashMap<String, String>>,
}
impl Job {
    pub fn runner_os(&self) -> RunnerOS {
        match &self.runner {
            Runner::Machine{ os, .. } => *os,
            Runner::Container{ image: _ } => RunnerOS::Linux
        }
    }
    pub fn runs_on_machine(&self) -> bool {
        match &self.runner {
            Runner::Machine{ .. } => true,
            Runner::Container{ image: _ } => false
        }
    }
    pub fn job_env<'a>(&'a self, config: &'a Config, system: &SystemJobEnvOptions) -> HashMap<&'a str, String> {
        let ci = config.ci_service_by_job(&self).unwrap();
        let env = ci.job_env();
        let mut common_envs = hashmap!{
            "DEPLO_JOB_CURRENT_NAME" => system.job_name.clone()
        };
        match config.runtime.release_target {
            Some(ref v) => {
                log::info!("job_env: release target: {}", v);
            },
            None => {
                let (ref_type, ref_path) = config.vcs_service().unwrap().current_ref().unwrap();
                log::info!("job_env: no release target: {}/{}", ref_type, ref_path);
            }
        };
        match system.paths {
            Some(ref paths) => {
                // modify path
                let mut paths = paths.clone();
                let path = std::env::var("PATH");
                match path {
                    Ok(v) => {
                        paths.push(v);
                    },
                    Err(_) => {}
                };
                common_envs.insert("PATH", paths.join(":"));
                log::debug!("modified path: {}", paths.join(":"));
            },
            None => {}
        };
        let mut h = env.clone();
        return match &self.env {
            Some(v) => {
                h.extend(common_envs);
                h.extend(v.iter().map(|(k,v)| (k.as_str(), v.to_string())));
                h
            },
            None => {
                h.extend(common_envs);
                h
            }
        };
    }
    pub fn matches_current_release_target(&self, target: &Option<String>) -> bool {
        let t = match target {
            Some(ref v) => v,
            // if no target, always ok if for_targets is empty, otherwise not ok
            None => return self.for_targets.is_none()
        };
        match &self.for_targets {
            Some(ref v) => {
                // here, both target and for_targets exists, compare matches
                for target in v {
                    if target == t {
                        return true;
                    }
                }
                return false;
            },
            None => {
                // target exists, but for_targets is empty, always ok
                return true;
            }
        }
    }
    pub fn commit_setting_from_release_target(&self, target: &Option<String>) -> Option<&Commit> {
        match &self.commits {
            Some(ref v) => {
                for commit in v {
                    match commit.for_targets {
                        // first only matches commit entry that has valid for_targets and target
                        Some(ref ts) => {
                            for t in ts {
                                if target.is_some() && t == target.as_ref().unwrap() {
                                    return Some(commit);
                                }
                            }
                        },
                        None => {}
                    }                    
                }
                // if no matches for all for_targets of Some, find first for_targets of None
                for commit in v {
                    match commit.for_targets {
                        // first only matches some for_targets and target
                        Some(_) => {},
                        None => return Some(commit)
                    }
                }
                // if no none, and matches for_targets, return none
                return None;
            },
            // if no commits setting, return none
            None => return None
        }
    }
    pub fn diff_matcher<'a>(&'a self) -> vcs::DiffMatcher<'a> {
        match self.options.as_ref().map_or_else(|| "glob", |v| v.get("diff_matcher").map_or_else(|| "glob", |v| v.as_str())) {
            "regex" => vcs::DiffMatcher::Regex(self.patterns.iter().map(|v| v.as_str()).collect()),
            "glob" => vcs::DiffMatcher::Glob(self.patterns.iter().map(|v| v.as_str()).collect()),
            others => panic!("unsupported diff matcher {}", others)
        }
    }
}
#[derive(Serialize, Deserialize, Clone, Copy, Eq, PartialEq)]
pub enum WorkflowType {
    Deploy,
    Integrate,
}
impl WorkflowType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "deploy" => Self::Deploy,
            "integrate" => Self::Integrate,
            _ => panic!("unknown workflow type: {}", s),
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Deploy => "deploy",
            Self::Integrate => "integrate",
        }
    }
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
    pub invoke_for_all_branches: Option<bool>,
}
#[derive(Serialize, Deserialize)]
pub struct Jobs {
    pub integrate: HashMap<String, Job>,
    pub deploy: HashMap<String, Job>,
}

#[derive(Serialize, Deserialize, Debug)]
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
#[serde(tag = "type", content = "patterns")]
pub enum ReleaseTarget {
    Branch(Vec<String>),
    Tag(Vec<String>),
}
impl ReleaseTarget {
    pub fn paths<'a>(&'a self) -> &'a Vec<String> {
        match self {
            Self::Branch(v) => v.as_ref(),
            Self::Tag(v) => v.as_ref(),
        }
    }
    pub fn is_branch(&self) -> bool {
        match self {
            Self::Branch(_) => true,
            _ => false,
        }
    }
    pub fn is_tag(&self) -> bool {
        match self {
            Self::Tag(_) => true,
            _ => false,
        }
    }
}
#[derive(Serialize, Deserialize)]
pub struct CommonConfig {
    pub project_name: String,
    pub data_dir: Option<String>,
    pub debug: Option<HashMap<String, String>>,
    pub release_targets: HashMap<String, ReleaseTarget>,
}
#[derive(Default)]
pub struct RuntimeConfig {
    pub verbosity: u64,
    pub dryrun: bool,
    pub debug: HashMap<String, String>,
    pub release_target: Option<String>,
    pub workflow_type: Option<WorkflowType>,
    pub workdir: Option<String>,
    pub dotenv_path: Option<String>,
    pub process_envs: HashMap<String, String>,
}
impl RuntimeConfig {
    fn with_args<A: args::Args>(args: &A) -> Self {
        RuntimeConfig {
            verbosity: match args.value_of("verbosity") {
                Some(o) => o.parse().unwrap_or(0),
                None => 0
            },
            dotenv_path: args.value_of("dotenv").map_or_else(|| None, |v| Some(v.to_string())),
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
            workflow_type: None, // set after
            workdir: args.value_of("workdir").map_or_else(|| None, |v| Some(v.to_string())),
            process_envs: hashmap!{}, // set after
        }
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
    fn post_apply<A: args::Args>(
        config: &mut Container, args: &A
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
                    None => return escalate!(Box::new(ConfigError{ 
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
                Some(WorkflowType::from_str(&v))
            } else {
                None
            },
            Err(_) => None
        };
        let workflow_type = match args.value_of("workflow-type") {
            Some(v) => Some(WorkflowType::from_str(v)),
            None => match env_workflow_type {
                // if DEPLO_OVERWRITE_WORKFLOW is set, behave as deploy workflow.
                Some(v) => Some(v),
                None => {
                    let immc = config.borrow();
                    let vcs = immc.vcs_service()?;
                    let (account_name, _) = immc.ci_config_by_env_or_default();
                    let ci = immc.ci_service(account_name)?;
                    match ci.pr_url_from_env()? {
                        Some(_) => Some(WorkflowType::Integrate),
                        None => match vcs.pr_url_from_env()? {
                            Some(_) => Some(WorkflowType::Integrate),
                            None => match vcs.release_target() {
                                Some(_) => Some(WorkflowType::Deploy),
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
    fn apply(&self) -> Result<(), Box<dyn Error>> {
        Self::setup_logger(self.verbosity);
        match self.workdir {
            Some(ref wd) => { 
                std::env::set_current_dir(wd)?; 
            },
            None => ()
        };
        // if the cli running on host, need to load dotenv to inject secrets
        if !Config::is_running_on_ci() {
            // load dotenv
            match self.dotenv_path {
                Some(ref dotenv_path) => match fs::metadata(dotenv_path) {
                    Ok(_) => { dotenv::from_filename(dotenv_path).unwrap(); },
                    Err(_) => { dotenv::from_readable(dotenv_path.as_bytes()).unwrap(); },
                },
                None => match dotenv() {
                    Ok(path) => log::debug!("using .env file at {}", path.to_string_lossy()),
                    Err(err) => if Config::is_running_on_ci() {
                        log::debug!("run on CI: environment variable is provided by CI system")
                    } else {
                        log::warn!("non-ci environment but .env not present or cannot load by error [{:?}], this usually means:\n\
                            1. command will be run with incorrect parameter or\n\
                            2. secrets are directly written in deplo.toml\n\
                            please use $repo/.env to provide secrets, or use -e flag to specify its path", err)
                    }
                },
            };
        };
        Ok(())
    }
}

pub type ConfigVersion = u64;
fn default_config_version() -> ConfigVersion {
    1
}
pub enum ConfigSource<'a> {
    File(&'a str),
    Memory(&'a str),
}
#[derive(Serialize, Deserialize)]
pub struct Config {
    // config from cli args
    #[serde(skip)]
    pub runtime: RuntimeConfig,
    // environment variables generated from environment variables given by CI service or localhost
    #[serde(skip)]
    pub process_envs: HashMap<String, String>,

    // object cache
    #[serde(skip)]
    pub ci_caches: HashMap<String, Box<dyn ci::CI>>,
    #[serde(skip)]
    pub vcs_cache: Vec<Box<dyn vcs::VCS>>,

    // config that loads from config file
    #[serde(default = "default_config_version")]
    pub version: ConfigVersion,
    pub common: CommonConfig,
    pub vcs: VCSConfig,
    pub ci: CIConfig,
    pub jobs: Jobs,
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
}

impl Config {
    // static factory methods 
    pub fn load(src: ConfigSource) -> Result<Config, Box<dyn Error>> {
        let src = match src {
            ConfigSource::File(path) => match fs::read_to_string(path) {
                Ok(v) => v,
                Err(e) => panic!("cannot read config at {}, err: {:?}", path, e)
            },
            ConfigSource::Memory(v) => v.to_string(),
        };
        let content = envsubst(&src);
        match toml::from_str(&content) {
            Ok(c) => Ok(c),
            Err(err) => escalate!(Box::new(err))
        }
    }
    pub fn with_config(
        src: ConfigSource,
        runtime_config: RuntimeConfig        
    ) -> Result<Container, Box<dyn Error>> {
        let c = Container{
            ptr: Rc::new(RefCell::new(
                Config::load(src).unwrap()
            ))
        };
        {
            let mut mutc = c.ptr.borrow_mut();
            mutc.runtime = runtime_config;
        }
        return Ok(c);
    }
    pub fn dummy(src: Option<&str>) -> Result<Container, Box<dyn Error>> {
        Self::with_config(
            ConfigSource::Memory(src.unwrap_or(include_str!("../res/test/dummy-Deplo.toml"))), 
            RuntimeConfig::default()
        )
    }
    pub fn create<A: args::Args>(args: &A) -> Result<Container, Box<dyn Error>> {
        let runtime_config = RuntimeConfig::with_args(args);
        // phase1. apply command line args that is essential for following phases
        runtime_config.apply()?;
        // phase2. load config file into this object
        let mut c = Self::with_config(
            ConfigSource::File(args.value_of("config").unwrap_or("Deplo.toml")),
            runtime_config
        )?;
        // phase3. setup module cache
        Self::setup_ci(&c)?;
        Self::setup_vcs(&c)?;
        // phase4. apply command line args which requires module cache existences
        // here, all command line argument should be applied.
        RuntimeConfig::post_apply(&mut c, args)?;
        // phase5. generate process environment.
        let envs = c.borrow().setup_process_env()?;
        c.borrow_mut().runtime.process_envs = envs;
        return Ok(c);
    }
    pub fn setup_process_env(&self) -> Result<HashMap<String, String>, Box<dyn Error>> {
        let (ci, local) = match self.ci_service_by_env() {
            Ok(ci) => (ci, false),
            Err(_) => (self.ci_service_main()?, true)
        };
        let vcs = self.vcs_service()?;
        let (ref_type, ref_path) = vcs.current_ref()?;
        let (may_tag, may_branch) = if ref_type == vcs::RefType::Tag {
            (Some(ref_path.as_str()), None)
        } else {
            (None, Some(ref_path.as_str()))
        };
        let sha = vcs.commit_hash(None)?;
        let random_id = randombytes_as_string!(16);
        let may_parent_ci_id = std::env::var("DEPLO_CI_ID");
        let ci_type = match self.current_ci_type_by_env() {
            Ok(v) => v,
            Err(_) => self.ci.accounts.get("default").unwrap().type_as_str().to_string(),
        };
        let mut default_envs = hashmap!{
            // on local, CI ID should be inherited from parent, if exists.
            // on CI DEPLO_CI_ID replaced with CI specific environment variable that represents canonical ID
            "DEPLO_CI_ID" => Some(match may_parent_ci_id {
                Ok(ref v) => v.as_str(),
                Err(_) => random_id.as_str(),
            }),
            // other CI process env should be recalculated, because user may call deplo on deferent environment.
            // but on CI, some of these variables may replaced by CI specific way, by return values of ci.process_env
            "DEPLO_CI_TYPE" => Some(ci_type.as_str()),
            "DEPLO_CI_TAG_NAME" => may_tag,
            "DEPLO_CI_BRANCH_NAME" => may_branch,
            "DEPLO_CI_CURRENT_COMMIT_ID" => Some(sha.as_str()),
            "DEPLO_CI_RELEASE_TARGET" => self.runtime.release_target.as_ref().map(|s| s.as_str()),
            "DEPLO_CI_WORKFLOW_NAME" => self.runtime.workflow_type.as_ref().map(|s| s.as_str()),
            // TODO_CI: get pull request url from local execution
            "DEPLO_CI_PULL_REQUEST_URL" => Some(""),
            "DEPLO_CI_CLI_COMMIT_HASH" => Some(DEPLO_GIT_HASH),
            "DEPLO_CI_CLI_VERSION" => Some(DEPLO_VERSION),
        };
        let envs = ci.process_env(local)?;
        for (k, v) in &envs {
            default_envs.insert(k, Some(v.as_str()));
        }
        let mut applied_process_envs = hashmap!{};
        for (k, v) in &default_envs {
            match v {
                Some(v) => {
                    std::env::set_var(k, v);
                    applied_process_envs.insert(k.to_string(), v.to_string());
                },
                None => std::env::remove_var(k),
            }
        };
        Ok(applied_process_envs)
    }
    pub fn data_dir<'a>(&'a self) -> &'a str {
        return match self.common.data_dir {
            Some(ref v) => v.as_str(),
            None => ".deplo"
        };
    }
    pub fn project_name(&self) -> &str {
        &self.common.project_name
    }
    pub fn runtime_release_target(&self) -> Option<&str> {
        match &self.runtime.release_target {
            Some(s) => Some(&s),
            None => None
        }
    }
    pub fn is_running_on_ci() -> bool {
        match std::env::var("CI") {
            Ok(v) => !v.is_empty(),
            Err(_) => false
        }
    }
    pub fn ci_cli_options(&self) -> String {
        let wdref = self.runtime.workdir.as_ref();
        return format!("{}", 
            if wdref.is_none() { "".to_string() } else { format!("-w {}", wdref.unwrap()) }
        );
    }
    pub fn generate_wrapper_script(&self) -> String {
        format!(
            include_str!("../res/cli/deplow.sh.tmpl"),
            version = DEPLO_VERSION,
            data_dir = self.data_dir()
        )
    }
    pub fn deplo_data_path(&self) -> Result<PathBuf, Box<dyn Error>> {
        let base = self.data_dir();
        let path = make_absolute(base, self.vcs_service()?.repository_root()?);
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
    pub fn deplo_cli_download(&self, os: RunnerOS, shell: &impl shell::Shell) -> Result<PathBuf, Box<dyn Error>> {
        match std::env::var("DEPLO_DEBUG_CLI_BIN_PATHS") {
            Ok(p) => {
                match serde_json::from_str::<HashMap<String, String>>(&p) {
                    Ok(m) => {
                        if let Some(v) = m.get(&os.to_string()) {
                            let path = make_absolute(v, self.vcs_service()?.repository_root()?);
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
        shell.download(&cli_download_url(os, DEPLO_VERSION), &file_path.to_str().unwrap(), true)?;
        return Ok(file_path);
    }
    pub fn parse_dotenv<F>(&self, mut cb: F) -> Result<(), Box<dyn Error>>
    where F: FnMut (&str, &str) -> Result<(), Box<dyn Error>> {
        if Self::is_running_on_ci() {
            // on ci, no dotenv file and secrets are already set as environment variable.
            // so we need to get secret name list from ci service and get their value from environment.
            // then pass key and vlaue to the closure cb.
            let (name, _) = self.ci_config_by_env_or_default();
            for secret in &self.ci_service(name)?.list_secret_name()? {
                match std::env::var(secret) {
                    Ok(v) => cb(secret, &v)?,
                    Err(_) => {}
                }
            }
        } else {
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
        }
        return Ok(())
    }
    pub fn current_ci_type_by_env(&self) -> Result<String, ConfigError> {
        let cis = hashmap!{
            "CIRCLE_SHA1" => "CircleCI",
            "GITHUB_ACTION" => "GhAction"
        };
        match std::env::var("DEPLO_CI_TYPE") {
            Ok(v) => return Ok(v),
            Err(_) => {
                for (key, value) in &cis {
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
    pub fn ci_config_by_env<'b>(&'b self) -> Option<(&'b str, &'b CIAccount)> {
        match self.current_ci_type_by_env() {
            Ok(t) => {
                for (account_name, account) in &self.ci.accounts {
                    if account.type_matched(&t) { return Some((account_name, account)) }
                }
            },
            Err(_) => {}
        };
        None
    }
    pub fn ci_config_by_env_or_default<'b>(&'b self) -> (&'b str, &'b CIAccount) {
        match self.ci_config_by_env() {
            Some(config) => config,
            None => ("default", self.ci.accounts.get("default").unwrap())
        }
    }
    pub fn ci_service<'a>(&'a self, account_name: &str) -> Result<&'a Box<dyn ci::CI>, Box<dyn Error>> {
        return match self.ci_caches.get(account_name) {
            Some(ci) => Ok(ci),
            None => escalate!(Box::new(ConfigError{ 
                cause: format!("no ci service for {}", account_name) 
            }))
        } 
    }
    pub fn ci_service_by_env<'a>(&'a self) -> Result<&'a Box<dyn ci::CI>, Box<dyn Error>> {
        match self.ci_config_by_env() {
            Some((account_name, _)) => match self.ci_caches.get(account_name) {
                Some(ci) => Ok(ci),
                None => escalate!(Box::new(ConfigError{
                    cause: format!("no ci service for {}", account_name)
                }))
            },
            None => {
                return escalate!(Box::new(ConfigError{
                    cause: "you don't set CI type and deplo cannot detect it. abort".to_string()
                }))
            }
        } 
    }
    pub fn ci_service_main<'a>(&'a self) -> Result<&'a Box<dyn ci::CI>, Box<dyn Error>> {
        return match self.ci_caches.get("default") {
            Some(ci) => Ok(ci),
            None => escalate!(Box::new(ConfigError{ 
                cause: format!("no ci service for default") 
            }))
        } 
    }
    pub fn ci_service_by_job_name<'a>(&'a self, job_name: &str) -> Result<&'a Box<dyn ci::CI>, Box<dyn Error>> {
        return match self.find_job(job_name) {
            Some(job) => self.ci_service_by_job(job),
            None => escalate!(Box::new(ConfigError{ 
                cause: format!("no such job {}", job_name) 
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
    pub fn job_env(
        &self, job: &Job, system: &SystemJobEnvOptions,
        adhoc: &HashMap<String, String>
    ) -> Result<HashMap<String, String>, Box<dyn Error>> {
        let mut inherits: HashMap<String, String> = if Self::is_running_on_ci() {
            shell::inherit_env()
        } else {
            let mut inherits_from_dotenv = HashMap::new();
            self.parse_dotenv(|k, v| {
                inherits_from_dotenv.insert(k.to_string(), v.to_string());
                Ok(())
            })?;
            for (k, v) in &self.runtime.process_envs {
                inherits_from_dotenv.insert(k.to_string(), v.to_string());
            }
            inherits_from_dotenv
        };
        for (k, v) in job.job_env(self, system) {
            inherits.insert(k.to_string(), v.to_string());
        }
        for (k, v) in adhoc {
            inherits.insert(k.to_string(), v.to_string());
        }
        return Ok(inherits);
    
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
    pub fn vcs_service_mut<'a>(&'a mut self) -> Result<&'a mut Box<dyn vcs::VCS>, Box<dyn Error>> {
        return if self.vcs_cache.len() > 0 {
            Ok(&mut self.vcs_cache[0])
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
    pub fn enumerate_jobs<'a>(&'a self) -> HashMap<(&'a str, &'a str), &'a Job> {
        let mut related_jobs: HashMap<(&'a str, &'a str), &'a Job> = hashmap!{};
        for (kind, jobs) in hashmap!{
            "integrate" => &self.jobs.integrate,
            "deploy" => &self.jobs.deploy
        } {
            for (name, job) in jobs {
                match related_jobs.insert((kind, name), job) {
                    None => {},
                    Some(_) => panic!("duplicated job name for {}: {}", kind, name)
                }
            }
        }
        return related_jobs;
    }
    pub fn find_job<'a>(&'a self, name: &str) -> Option<&'a Job> {
        let tuple = match name.find("-").map(|i| name.split_at(i)).map(|(k, v)| (k, v.split_at(1).1)) {
            Some((kind, name)) => (kind, name),
            None => return None
        };
        match self.enumerate_jobs().get(&tuple) {
            Some(job) => return Some(job),
            None => return None
        }
    }
    pub fn adjust_commit_hash(&self, commit: &Option<&str>) -> Result<(), Box<dyn Error>> {
        if !Self::is_running_on_ci() {
            if let Some(ref c) = commit {
                log::debug!("change commit hash to {}", c);
                self.vcs_service()?.checkout(c, Some(DEPLO_VCS_TEMPORARY_WORKSPACE_NAME))?;
            }
        }
        Ok(())
    }
    pub fn recover_branch(&self) -> Result<(), Box<dyn Error>> {
        if !Self::is_running_on_ci() {
            let vcs = self.vcs_service()?;
            let (ref_type, ref_path) = vcs.current_ref()?;
            if ref_type == vcs::RefType::Branch &&
                ref_path == DEPLO_VCS_TEMPORARY_WORKSPACE_NAME {
                log::debug!("back to previous branch");
                vcs.checkout_previous()?;
            }
        }
        Ok(())
    }
    pub fn run_job_by_name(
        &self, shell: &impl shell::Shell, name: &str, 
        command: Command, options: &JobRunningOptions
    ) -> Result<Option<String>, Box<dyn Error>> {
        match self.find_job(name) {
            Some(job) => self.run_job(shell, name, job, command, options),
            None => return escalate!(Box::new(
                ConfigError{ cause: format!("job {} not found", name) }
            )),
        }
    }
    fn create_steps<'a>(&self, job: &'a Job, command: &'a Command) -> (Vec<Step>, Option<&'a str>){   
        match command {
            Command::Adhoc(ref c) => {
                (vec![Step::Command{ name: None, command: c.to_string(), env: None, workdir: None, shell: job.shell.clone() }], Some(c.as_str()))
            },
            Command::Job => if let Some(steps) = &job.steps {
                (steps.to_vec(), None)
            } else if let Some(c) = &job.command {
                (vec![Step::Command{ name: None, command: c.to_string(), env: None, workdir: None, shell: job.shell.clone() }], Some(c.as_str()))
            } else {
                panic!("neither job.command nor job.steps specified");
            },
            Command::Shell => {
                let c = job.shell.as_ref().map_or_else(|| "bash", |v| v.as_str());
                (vec![Step::Command{ name: None, command: c.to_string(), env: None, workdir: None, shell: job.shell.clone()}], Some(c))
            }
        }
    }
    pub fn run_job_steps(
        &self, shell: &impl shell::Shell, shell_settings: &shell::Settings, job: &Job
    ) -> Result<Option<String>, Box<dyn Error>> {
        let (steps, _) = self.create_steps(job, &Command::Job);
        self.run_steps(shell, shell_settings, &shell::inherit_env(), job, &steps)
    }
    fn step_runner_command(name: &str) -> String {
        format!("deplo ci {} steps", name.replacen("-", " ", 1))
    }
    fn run_steps(
        &self, shell: &impl shell::Shell, shell_settings: &shell::Settings,
        base_envs: &HashMap<String, String>, job: &Job, steps: &Vec<Step>
    ) -> Result<Option<String>, Box<dyn Error>> {
        let empty_envs = hashmap!{};
        for step in steps {
            match step {
                Step::Command{shell: sh, command, env, workdir, name:_} => {
                    shell.eval(
                        command, sh, 
                        merge_hashmap(base_envs, &env.as_ref().map_or_else(|| &empty_envs, |v| v)),
                        &workdir.as_ref().map_or_else(|| job.workdir.as_ref(), |v| Some(v)), shell_settings
                    )?;
                },
                Step::Module{uses, with, name:_} => {
                    panic!("TODO: running step by module {} with {:?}", uses, with)
                }
            }
        };
        Ok(None)
    }
    pub fn run_job(
        &self, shell: &impl shell::Shell, name: &str, job: &Job, 
        command: Command, options: &JobRunningOptions
    ) -> Result<Option<String>, Box<dyn Error>> {
        let (steps, main_command) = self.create_steps(job, &command);
        // if current commit is modified, rollback after all operation is done.
        defer!{self.recover_branch().unwrap();};
        if options.remote {
            log::debug!(
                "force running job {} on remote with steps {} at {}", 
                name, StepsDumper{steps: &steps}, options.commit.unwrap_or("")
            );
            let ci = self.ci_service_by_job(job)?;
            let vcs = self.vcs_service()?;
            let commit = match options.commit {
                Some(c) => c.to_string(),
                None => vcs.commit_hash(None)?
            };
            return Ok(Some(ci.run_job(&ci::RemoteJob{
                name: name.to_string(),
                command: main_command.map(|v| v.to_string()),
                commit: Some(commit),
                envs: options.adhoc_envs.to_owned(),
                verbosity: self.runtime.verbosity,
                release_target: self.runtime.release_target.clone(),
                workflow: self.runtime.workflow_type.map(|w| w.as_str().to_string()),
            })?));
        }
        let mut system = SystemJobEnvOptions{
            paths: None,
            job_name: name.to_string(),
        };
        match job.runner {
            Runner::Machine{os, ref local_fallback, ..} => {
                let current_os = shell.detect_os()?;
                if os == current_os {
                    system.paths = if !Self::is_running_on_ci() {
                        let cli_parent_dir = self.deplo_cli_download(
                            os, shell
                        )?.parent().unwrap().to_string_lossy().to_string();
                        Some(vec![cli_parent_dir.to_owned()])
                    } else {
                        None
                    };
                    self.adjust_commit_hash(&options.commit)?;
                    // run command directly here, add path to locally downloaded cli.
                    self.run_steps(shell, &options.shell_settings, &self.job_env(&job, &system, &options.adhoc_envs)?, job, &steps)?;
                    self.post_run_job(name, &job)?;
                } else {
                    log::debug!("runner os is different from current os {} {}", os, current_os);
                    match local_fallback {
                        Some(f) => {
                            let (image, sh) = match f {
                                FallbackContainer::ImageUrl{ image, shell: sh } => (image.clone(), sh),
                                FallbackContainer::DockerFile{ path, shell: sh, repo_name } => {
                                    let local_image = match repo_name.as_ref() {
                                        Some(n) => format!("{}:{}", n, name),
                                        None => format!("{}-deplo-local-fallback:{}", self.common.project_name, name)
                                    };
                                    log::info!("generate fallback docker image {} from {}", local_image, path);
                                    let p = Path::new(path);
                                    shell.exec(
                                        &shell::args![
                                            "docker", "build", 
                                            "-t", &local_image, 
                                            "-f", p.file_name().unwrap().to_str().unwrap(),
                                            "."
                                        ], shell::no_env(),
                                        &Some(p.parent().unwrap().to_string_lossy().to_string()),
                                        &shell::capture()
                                    )?;
                                    (local_image, sh)
                                },
                            };
                            let path = &self.deplo_cli_download(os, shell)?.to_string_lossy().to_string();
                            self.adjust_commit_hash(&options.commit)?;
                            shell.eval_on_container(
                                &image,
                                // if main_command is none, we need to run steps in single container.
                                // so we execute `deplo ci steps $job_name` to run steps of $job_name.
                                &main_command.map_or_else(|| Self::step_runner_command(name), |v| v.to_string()),
                                &sh, self.job_env(&job, &system, &options.adhoc_envs)?,
                                &job.workdir, &hashmap!{
                                    path.as_str() => "/usr/local/bin/deplo"
                                }, &options.shell_settings
                            )?;
                            self.post_run_job(name, &job)?;
                            return Ok(None);
                        },
                        None => ()
                    };
                    // runner os is not linux and not same as current os, and no fallback container specified.
                    // need to run in CI.
                    let ci = self.ci_service_by_job(job)?;
                    let vcs = self.vcs_service()?;
                    let commit = match options.commit {
                        Some(c) => c.to_string(),
                        None => vcs.commit_hash(None)?
                    };        
                    return Ok(Some(ci.run_job(&ci::RemoteJob{
                        name: name.to_string(),
                        command: main_command.map(|v| v.to_string()),
                        commit: Some(commit),
                        envs: hashmap!{},
                        verbosity: self.runtime.verbosity,
                        release_target: self.runtime.release_target.clone(),
                        workflow: self.runtime.workflow_type.map(|w| w.as_str().to_string()),
                    })?));
                }
            },
            Runner::Container{ ref image } => {
                self.adjust_commit_hash(&options.commit)?;
                if Self::is_running_on_ci() {
                    // already run inside container `image`, run command directly here
                    self.run_steps(shell, &options.shell_settings, &self.job_env(&job, &system, &options.adhoc_envs)?, job, &steps)?;
                } else {
                    let os = RunnerOS::Linux;
                    let path = &self.deplo_cli_download(os, shell)?.to_string_lossy().to_string();
                    // running on host. run command in container `image` with docker
                    shell.eval_on_container(
                        image,
                        // if main_command is none, we need to run steps in single container. 
                        // so we execute `deplo ci steps $job_name` to run steps of $job_name.
                        &main_command.map_or_else(|| Self::step_runner_command(name), |v| v.to_string()),
                        &job.shell, self.job_env(&job, &system, &options.adhoc_envs)?,
                        &job.workdir, &hashmap!{
                            path.as_str() => "/usr/local/bin/deplo"
                        }, &options.shell_settings
                    )?;
                }
                self.post_run_job(name, &job)?;
            }
        }
        Ok(None)
    }
    pub fn post_run_job(&self, job_name: &str, job: &Job) -> Result<(), Box<dyn Error>> {
        let mut system_job_outputs = hashmap!{};
        match job.commit_setting_from_release_target(&self.runtime.release_target) {
            Some(commit) => {
                let vcs = self.vcs_service()?;
                match vcs.current_ref()? {
                    (vcs::RefType::Branch|vcs::RefType::Pull, _) => {
                        let branch_name = format!(
                            "deplo-auto-commits-{}-tmp-{}", 
                            crate::util::env::var_or_die("DEPLO_CI_ID"),
                            job_name
                        );
                        if vcs.push_diff(
                            // basically the branch_name does not exists in remote,
                            // we need to add refs/heads to create it automatically
                            &format!("refs/heads/{}", branch_name), 
                            &commit.generate_commit_log(job_name, &job),
                            &commit.patterns.iter().map(AsRef::as_ref).collect::<Vec<&str>>(),
                            &hashmap!{}
                        )? {
                            system_job_outputs.insert(DEPLO_SYSTEM_OUTPUT_COMMIT_BRANCH_NAME, branch_name);
                        }
                    },
                    (ty, b) => {
                        log::warn!("current ref is not a branch ({}/{}), skip auto commit", ty, b);
                    }
                }
            },
            None => { 
                log::debug!(
                    "no commit settings for release target {}", 
                    self.runtime.release_target.as_ref().map_or_else(|| "none", |v| v.as_str())
                );
            }
        };
        let ci = self.ci_service_by_job_name(job_name)?;
        if system_job_outputs.len() > 0 {
            log::debug!("set system job outputs: {:?}", system_job_outputs);
            ci.set_job_output(
                job_name, ci::OutputKind::System, 
                system_job_outputs.iter().map(|(k,v)| (*k, v.as_str())).collect()
            )?;
            ci.mark_need_cleanup(job_name)?;
        }
        match fs::read(Path::new(DEPLO_JOB_OUTPUT_TEMPORARY_FILE)) {
            Ok(b) => {
                let outputs = serde_json::from_slice::<HashMap<&str, &str>>(&b)?;
                log::debug!("set user job outputs: {:?}", outputs);
                ci.set_job_output(job_name, ci::OutputKind::User, outputs)?;
                rm(DEPLO_JOB_OUTPUT_TEMPORARY_FILE);
            },
            Err(_) => {}
        }
        Ok(())
    }
    pub fn set_user_job_output(&self, key: &str, value: &str) -> Result<(), Box<dyn Error>> {
        match fs::read(Path::new(DEPLO_JOB_OUTPUT_TEMPORARY_FILE)) {
            Ok(b) => {
                let mut outputs = serde_json::from_slice::<HashMap<&str, &str>>(&b)?;
                outputs.insert(key, value);
                fs::write(DEPLO_JOB_OUTPUT_TEMPORARY_FILE, serde_json::to_string(&outputs)?)?;
            },
            Err(_) => {
                fs::write(DEPLO_JOB_OUTPUT_TEMPORARY_FILE, serde_json::to_string(&hashmap!{ key => value })?)?;
            }
        };
        Ok(())
    }
    pub fn user_job_output(&self, job_name: &str, key: &str) -> Result<Option<String>, Box<dyn Error>> {
        let ci = self.ci_service_by_job_name(job_name)?;
        match std::env::var("DEPLO_JOB_CURRENT_NAME") {
            Ok(n) => {
                if n == job_name {
                    // get output of current job. read from temporary file
                    match fs::read(Path::new(DEPLO_JOB_OUTPUT_TEMPORARY_FILE)) {
                        Ok(b) => {
                            let outputs = serde_json::from_slice::<HashMap<&str, &str>>(&b)?;
                            Ok(outputs.get(key).map(|v| v.to_string()))
                        },
                        Err(e) => escalate!(Box::new(e))
                    }
                } else {
                    ci.job_output(job_name, ci::OutputKind::User, key)
                }
            },
            Err(_) => ci.job_output(job_name, ci::OutputKind::User, key)
        }
    }
    pub fn system_job_output(&self, job_name: &str, key: &str) -> Result<Option<String>, Box<dyn Error>> {
        let ci = self.ci_service_by_job_name(job_name)?;
        ci.job_output(job_name, ci::OutputKind::System, key)
    }
    pub fn is_main_ci(&self, ci_type: &str) -> bool {
        // default always should exist
        return self.ci.accounts.get("default").unwrap().type_matched(ci_type);
    }
}
