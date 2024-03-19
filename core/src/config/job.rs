use std::collections::{HashMap};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path};
use std::io::Write;
use std::thread::sleep;
use std::time::Duration;

use maplit::hashmap;
use petgraph;
use serde::{Deserialize, Serialize};

use crate::ci;
use crate::config;
use crate::shell;
use crate::util::{escalate,UnitOrListOf,merge_hashmap};
use crate::vcs;

pub mod runner;

pub const DEPLO_JOB_OUTPUT_TEMPORARY_FILE: &'static str = "deplo-tmp-job-output.json";
pub const DEPLO_SYSTEM_OUTPUT_COMMIT_BRANCH_NAME: &'static str = "COMMIT_BRANCH";

/// represents single cache setting of CI service.
#[derive(Serialize, Deserialize)]
pub struct Cache {
    /// hierarchical names of the cache. each CI system uses these names to determine the cache already exists or not.
    pub keys: Vec<config::Value>,
    /// paths that stores into the cache.
    pub paths: Vec<config::Value>
}
/// configuration for local execution of machine runner type job.
/// because machine runner is use VM environment of CI service, it is different from local one.
/// for example, local env does not install cli that CI service VM environemnt does.
/// to absorb the difference, you can specify docker image or dockerfile to run the job
/// FallbackContainer is represent such a configuration.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContainerImageSource {
    /// docker image that is used for local execution.
    ImageUrl{ image: config::Value },
    /// dockerfile that is used for local execution. deplo build docker iamge with the dockerfile.
    DockerFile{ path: config::Value, repo_name: Option<config::Value> },
}
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum Input {
    Path(config::Value),
    List {
        includes: Vec<config::Value>,
        excludes: Vec<config::Value>
    }
}
#[derive(Serialize, Deserialize)]
pub struct FallbackContainer {
    #[serde(flatten)]
    source: ContainerImageSource,
    shell: Option<config::Value>,
    pub inputs: Option<Vec<Input>>,
    pub caches: Option<Vec<config::Value>>
}
/// configuration of os of machine type runner
#[derive(Serialize, Deserialize, Clone, Copy, Eq, PartialEq, Hash)]
// this annotation and below impl TryFrom<String> are 
// required because RunnerOS is used as HashMap key.
// see https://stackoverflow.com/a/68580953/1982282 for detail
#[serde(try_from = "String")]
pub enum RunnerOS {
    #[serde(rename = "linux")]
    Linux,
    #[serde(rename = "windows")]
    Windows,
    #[serde(rename = "macos")]
    MacOS,
}
impl RunnerOS {
    pub fn from_str(s: &str) -> Result<Self, Box<dyn Error>> {
        match s {
            "linux" => Ok(Self::Linux),
            "windows" => Ok(Self::Windows),
            "macos" => Ok(Self::MacOS),
            _ => escalate!(Box::new(config::ConfigError{cause: format!("no such os: [{}]", s)})),
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
            Self::Linux => match std::env::consts::ARCH {
                "x86_64"|"amd64" => "Linux-x86_64",
                "aarch64"|"arm64" => "Linux-aarch64",
                _ => panic!("unsupported cpu arch {}", std::env::consts::ARCH)
            },
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
impl TryFrom<String> for RunnerOS {
    type Error = Box<dyn Error>;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        RunnerOS::from_str(s.as_str())
    }
}
/// configuration for runner of the job.
/// machine runner is use VM environment of CI service. less compatibility of local execution but faster.
/// container runner is use exactly same container for local execution as CI service. 
/// maximum compability of local execution but additional time to invoke job on CI service (for pulling image).
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum Runner {
    #[serde(rename = "machine")]
    Machine {
        /// os to run the job.
        os: RunnerOS,
        /// image of the VM to run the job.
        image: Option<config::Value>,
        /// spec of the VM to run the job.
        class: Option<config::Value>,
        /// container image setting for run the job on local.
        local_fallback: Option<FallbackContainer>,
    },
    #[serde(rename = "container")]
    Container {
        /// container image to run the job.
        image: config::Value,
        inputs: Option<Vec<Input>>,
    }
}
/// command execution pattern
#[derive(Eq, PartialEq)]
pub enum Command {
    /// invoke single step that only contains command specified with the String
    Adhoc(String),
    /// invoke multiple steps, job.steps specified, otherwise job.command executed.
    Job,
    /// invoke interactive shell using job.shell value. if job.runner.local_fallback.shell is specified, this value is used.
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
/// represents the way to commit artifact of jobs.
/// push and pull request are supported.
/// each job's commit are aggregated by final CI job, called deplo-cleanup.
/// and separated to 4 group
/// 1. Push & squash = true => squashed to single commit and pushed to built branch
/// 2. Push & squash = false => pushed to built branch as thease are
/// 3. Pull request & aggregate = true => all commits are pushed to single working branch and pull request is made to built branch
/// 4. Pull request & aggregate = false => each commits are pushed to seprated working branch and separated pull request is made to built branch
#[derive(Serialize, Deserialize)]
#[serde(tag = "with")]
pub enum CommitMethod {
    #[serde(rename = "push")]
    Push {
        /// whether pushed commits are squashed.
        squash: Option<bool>,
    },
    #[serde(rename = "pull_request")]
    PullRequest {
        /// labels of the pull request that is created.
        labels: Option<Vec<config::Value>>,
        /// assignees of the pull request that is created.
        assignees: Option<Vec<config::Value>>,
        /// whether separated pull request is made for the commit
        aggregate: Option<bool>,
    }
}
#[derive(Serialize, Deserialize)]
pub struct Commit {
    pub files: Vec<config::Value>,
    pub on: Option<Trigger>,
    pub log_format: Option<config::Value>,
    #[serde(flatten)]
    pub method: Option<CommitMethod>,
}
impl Commit {
    fn generate_commit_log(&self, name: &str, _: &Job) -> String {
        //TODO: pass variable to log_format
        self.log_format.as_ref().map_or_else(
            || format!("[deplo] update by job {job_name}", job_name = name),
            |v| v.resolve().to_string()
        )
    }
}
#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum StepCommand {
    Eval {
        command: config::Value,
        shell: Option<config::Value>,
        workdir: Option<config::Value>,
    },
    Exec {
        exec: Vec<config::Value>,
        workdir: Option<config::Value>,
    },
    Module(config::module::ConfigFor<crate::step::ModuleDescription>)
}
#[derive(Serialize, Deserialize, Clone)]
pub struct Step {
    pub name: Option<String>,
    pub env: Option<HashMap<String, config::Value>>,
    #[serde(flatten)]
    pub command: StepCommand,
}
impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.command {
            StepCommand::Eval{
                command, shell, workdir
            } => write!(f,
                "Step::Command{{name:{}, command:{}, env:{}, shell:{}, workdir:{}}}",
                self.name.as_ref().map_or_else(|| "".to_string(), |s| s.to_string()),
                command,
                self.env.as_ref().map_or_else(|| "None".to_string(), |e| format!("{:?}", e)),
                shell.as_ref().map_or_else(|| "None".to_string(), |e| format!("{:?}", e)),
                workdir.as_ref().map_or_else(|| "None".to_string(), |e| format!("{:?}", e)),
            ),
            StepCommand::Exec{
                exec, workdir
            } => write!(f,
                "Step::Command{{name:{}, exec:{:?}, env:{}, workdir:{}}}",
                self.name.as_ref().map_or_else(|| "".to_string(), |s| s.to_string()),
                exec,
                self.env.as_ref().map_or_else(|| "None".to_string(), |e| format!("{:?}", e)),
                workdir.as_ref().map_or_else(|| "None".to_string(), |e| format!("{:?}", e)),
            ),
            StepCommand::Module(c) => c.value(|v| write!(f,
                "Step::Module{{name:{}, uses:{}, with:{}",
                self.name.as_ref().map_or_else(|| "".to_string(), |s| s.to_string()),
                v.uses.to_string(),
                v.with.as_ref().map_or_else(|| "None".to_string(), |e| format!("{:?}", e))
            ))
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
#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum TriggerCondition {
    Cron {
        schedules: Vec<config::Value>,
    },
    Repository {
        events: Vec<config::Value>,
    },
    Module {
        when: HashMap<String, config::AnyValue>,
    },
    Commit {
        changed: Vec<config::Value>,
        diff_matcher: Option<config::Value>,
    },
    Any {
        any: Option<()>
    }
}
impl TriggerCondition {
    fn check_workflow_type(&self, w: &config::workflow::Workflow) -> bool {
        match self {
            Self::Commit{..} => if let config::workflow::Workflow::Deploy{..} = w {
                true
            } else if let config::workflow::Workflow::Integrate{..} = w {
                true
            } else {
                false
            },
            Self::Cron {..} => if let config::workflow::Workflow::Cron{..} = w {
                true
            } else {
                false
            },
            Self::Repository{..} => if let config::workflow::Workflow::Repository{..} = w {
                true
            } else {
                false
            },
            Self::Module{..} => if let config::workflow::Workflow::Module{..} = w {
                true
            } else {
                false
            },
            // no workflow type specific condition. always returns true
            Self::Any{..} => true        
        }
    }
}
#[derive(Serialize, Deserialize)]
pub struct Trigger {
    workflows: Option<Vec<config::Value>>,
    release_targets: Option<Vec<config::Value>>,
    #[serde(flatten)]
    condition: TriggerCondition,
}
impl Trigger {
    pub fn diff_matcher(
        matcher: &Option<config::Value>, 
        patterns: &Vec<config::Value>
    ) -> vcs::DiffMatcher {
        let matcher_type = matcher.as_ref().map_or_else(|| "glob".to_string(), |v| v.resolve());
        match matcher_type.as_str() {
            "regex" => vcs::DiffMatcher::Regex(patterns.iter().map(config::Value::resolve_to_string).collect()),
            "glob" => vcs::DiffMatcher::Glob(patterns.iter().map(config::Value::resolve_to_string).collect()),
            others => panic!("unsupported diff matcher {}", others)
        }
    }
    pub fn matches(
        &self,
        job: &config::job::Job,
        config: &config::Config,
        runtime_workflow_config: &config::runtime::Workflow,
        options: Option<MatchOptions>
    ) -> bool {
        match &runtime_workflow_config.job {
            Some(j) => if j.name != job.name {
                log::debug!(
                    "workflow '{}' is running for single job '{}' but current job is '{}'",
                    runtime_workflow_config.name, j.name, job.name
                );
                return false;
            },
            None => {}
        }
        let opts = options.unwrap_or(MatchOptions::from(runtime_workflow_config));
        let workflow = config.workflows.as_map().get(&runtime_workflow_config.name).expect(
            &format!("{} does not exist in workflows of Deplo.toml", runtime_workflow_config.name)
        );
        // workflows match?
        if !self.workflows.as_ref().map_or_else(
            // when no workflow specified for condition, always pass
            || true,
            // if more than 1 workflows specified, runtime_workflow_config.name should match any of them
            |v| v.iter().find(|v| { let s: &str = &v.resolve(); s == runtime_workflow_config.name }).is_some()
        ) {
            log::debug!(
                "workflow '{}' does not match for trigger workflow '{:?}' of '{}'", 
                runtime_workflow_config.name, self.workflows, job.name
            );
            return false;
        }
        if !self.release_targets.as_ref().map_or_else(
            // no job.release_targets restriction. always ok
            || true,
            |v| match &runtime_workflow_config.exec.release_target {
                // both runtime_workflow_config.exec.release_target and job.release_targets exists, compare matches
                Some(rt) => v.iter().find(|v| { v.resolve() == *rt }).is_some(),
                // job.release_target exists, but no runtime_workflow_config.exec.release_target, always ng
                None => false
            }
        ) {
            log::debug!("workflow '{}' does not match for release target '{:?}' of '{}'. current release target is '{:?}'", 
                workflow, self.release_targets, job.name, runtime_workflow_config.exec.release_target);
            return false;
        }        
        if !self.condition.check_workflow_type(workflow) {
            log::debug!(
                "workflow '{}' does not match for trigger condition type '{:?}' of '{}'",
                workflow, self.condition, job.name
            );
            return false;
        }
        if !opts.check_condition {
            log::debug!("skip condition check for job '{}' in workflow '{}'", job.name, workflow);
            return true;
        }
        match &self.condition {
            TriggerCondition::Commit{ changed, diff_matcher } => {
                // diff pattern matches
                let dm = Self::diff_matcher(diff_matcher, changed);
                if !config.modules.vcs().changed(&dm) {
                    log::trace!(
                        "workflow '{}' job '{}' diff pattern {:?} does not match any of changed files in last commit", 
                        workflow, job.name, changed
                    );
                    return false;
                }
            },
            TriggerCondition::Cron{ schedules } => {
                let schedule = runtime_workflow_config.context.get("schedule").unwrap();
                if !schedules.iter().find(|s| { s.resolve() == schedule.resolve() }).is_some() {
                    log::trace!(
                        "workflow '{}' job '{}' schedule {} does not match any of schedules {:?}", 
                        workflow, job.name, schedule.resolve(), schedules
                    );
                    return false;
                }
            },
            TriggerCondition::Repository{ events } => {
                let repository_events = runtime_workflow_config.context.get("events").unwrap();
                if !repository_events.is_array() {
                    panic!("repository events not array {}", repository_events);
                }
                if !events.iter().find(|e| { 
                    repository_events.iter(|rev| {
                        if rev.resolve() == e.resolve() { Some(()) } else { None }
                    }).is_some()
                }).is_some() {
                    log::trace!(
                        "workflow '{}' job '{}' repository events {:?} does not match any of events {:?}",
                        workflow, job.name, repository_events, events
                    );
                    return false;
                }
            },
            TriggerCondition::Module{ when } => {
                // use module method like _wf.matches(when) to determine.
                panic!(
                    "TODO: workflow '{}' job '{}' implement match logic for Module condition {:?}",
                    workflow, job.name, when
                );
            },
            TriggerCondition::Any{..} => {},
        }
        log::trace!("trigger condition {:?} matches with workflow {}", self.condition, workflow);
        return true
    }
}
#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum SubmoduleCheckoutType {
    Checkout(bool),
    #[serde(rename = "recursive")]
    Recursive,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct CheckoutOption {
    pub lfs: Option<bool>,
    pub submodules: Option<SubmoduleCheckoutType>,
    pub fetch_depth: Option<u64>,
    pub token: Option<config::Value>
}
impl CheckoutOption {
    pub fn default() -> Self {
        Self {
            lfs: None,
            submodules: None,
            fetch_depth: None,
            token: None
        }
    }
    pub fn merge(&self, with: &Self) -> Self {
        Self {
            lfs: match with.lfs {
                Some(v) => Some(v),
                None => self.lfs
            },
            submodules: match with.submodules {
                Some(ref v) => Some(v.clone()),
                None => self.submodules.clone()
            },
            fetch_depth: match with.fetch_depth {
                Some(ref v) => Some(*v),
                None => self.fetch_depth
            },
            token: match with.token {
                Some(ref v) => Some(v.clone()),
                None => self.token.clone()
            },
        }
    }
}
#[derive(Serialize, Deserialize)]
pub struct Job {
    #[serde(skip, default)]
    pub name: String,
    pub account: Option<config::Value>,
    pub on: UnitOrListOf<Trigger>,
    pub runner: Runner,
    pub shell: Option<config::Value>,
    pub command: Option<config::Value>,
    pub steps: Option<Vec<Step>>,
    pub env: Option<HashMap<String, config::Value>>,
    pub workdir: Option<config::Value>,
    pub checkout: Option<CheckoutOption>,
    pub caches: Option<HashMap<String, Cache>>,
    pub depends: Option<Vec<config::Value>>,
    pub commit: Option<UnitOrListOf<Commit>>,
    pub options: Option<HashMap<String, config::AnyValue>>,
    // TODO: able to specify steps for tasks
    pub tasks: Option<HashMap<String, config::Value>>,
}
impl Job {
    pub fn runner_os(&self) -> RunnerOS {
        match &self.runner {
            Runner::Machine{ os, .. } => *os,
            Runner::Container{ .. } => RunnerOS::Linux
        }
    }
    pub fn runs_on_machine(&self) -> bool {
        match &self.runner {
            Runner::Machine{ .. } => true,
            Runner::Container{ .. } => false
        }
    }
    pub fn command_args<'a>(&self, may_args: Option<Vec<&'a str>>) -> Option<Vec<String>> {
        match may_args {
            Some(args) => if args.len() > 0 {
                if args[0].starts_with("@") {
                    let task = &(args[0][1..]);
                    let task_command = self.tasks.as_ref().
                        expect(&format!("no task definition for job {}", self.name)).
                        get(task).
                        expect(&format!("task definition for {} not found", task));
                    log::debug!("task '{}' resolved to [{}]", task, task_command.resolve());
                    Some(vec![task_command.resolve()])
                } else {
                    Some(args.iter().map(|vv| vv.to_string()).collect())
                }
            } else {
                None
            },
            None => None
        }
    }
    pub fn ci<'a>(&self, config: &'a config::Config) -> &Box<dyn ci::CI + 'a> {
        let account = self.account.as_ref().map_or_else(
            || "default".to_string(), config::Value::resolve_to_string
        );
        return config.modules.ci_for(&account);
    }
    pub fn run<S>(
        &self,
        shell: &S,
        config: &config::Config,
        runtime_workflow_config: &config::runtime::Workflow
    ) -> Result<Option<String>, Box<dyn Error>> where S: shell::Shell {
        let runner = runner::Runner::new(self, config);
        let r = runner.run(shell, runtime_workflow_config);
        crate::util::try_debug!(&self.name, self.ci(config), runtime_workflow_config.exec, r.is_err());
        // if runtime_workflow_config.exec.debug.should_start(r.is_err()) {
        //     log::info!("start debugger for job {} by result {:?}, config {}",
        //         self.name, r, runtime_workflow_config.exec.debug);
        // } else {
        //     let ci = self.ci(config);
        //     ci.set_job_env(hashmap!{
        //         "DEPLO_CI_RUN_DEBUGGER" => ""
        //     })?;
        // }
        r
    }
    pub fn env(
        &self,
        config: &config::Config,
        runtime_workflow_config: &config::runtime::Workflow
    ) -> HashMap<String, config::Value> {
        let ci = self.ci(config);
        let secrets = config::secret::as_config_values();
        let mut envs_list = vec![];
        envs_list.push(&config.envs);
        envs_list.push(&secrets);
        let common_envs = merge_hashmap(&hashmap!{
            "DEPLO_CI_JOB_NAME".to_string() => config::Value::new(&self.name),
            "DEPLO_CI_WORKFLOW_NAME".to_string() => config::Value::new(&runtime_workflow_config.name),
            "DEPLO_CI_WORKFLOW_CONTEXT".to_string() => config::Value::new(
                &match serde_json::to_string(&runtime_workflow_config.context) {
                    Ok(v) => v,
                    Err(e) => panic!(
                        "workflow context does not have valid form {:?} err:{:?}",
                        runtime_workflow_config.context, e
                    )
                }
            ),
        }, &match runtime_workflow_config.exec.release_target {
            Some(ref v) => hashmap!{"DEPLO_CI_RELEASE_TARGET".to_string() => config::Value::new(v)},
            None => hashmap!{}
        });
        let mut depend_envs = hashmap!{};
        if config::Config::is_running_on_ci() {
            match &self.depends {
                Some(ds) => {
                    for d in ds {
                        let envkey = ci::OutputKind::User.env_name_for_job(&d.resolve());
                        let envval = config::Value::new(&std::env::var(&envkey).expect(&format!("{} should set", &envkey)));
                        depend_envs.insert(envkey, envval);
                    }
                },
                None => {}
            };
        }
        envs_list.push(&depend_envs);
        envs_list.push(&common_envs);
        let jenvs = ci.job_env();
        envs_list.push(&jenvs);
        match &self.env {
            Some(v) => envs_list.push(v),
            None => {}
        };
        let exec_envs = runtime_workflow_config.exec.envs.iter().map(
            |(k,v)| (k.to_string(), config::Value::new(v))
        ).collect();
        envs_list.push(&exec_envs);
        let mut h = hashmap!{};
        for envs in envs_list {
            for (k,v) in envs {
                h.insert(k.clone(), v.clone());
            }
        }
        return h;
    }
    pub fn matches_current_trigger(
        &self,
        config: &config::Config,
        rtconfig: &config::runtime::Workflow
    ) -> bool {
        for t in &self.on {
            if t.matches(self, config, rtconfig, None) {
                return true
            }
        }
        return false
    }
    pub fn commit_setting_from_config(
        &self,
        config: &config::Config,
        runtime_workflow_config: &config::runtime::Workflow
    ) -> Option<&Commit> {
        match &self.commit {
            Some(ref v) => {
                for commit in v {
                    match commit.on {
                        // first only matches commit entry that has valid for_targets and target
                        Some(ref t) => if t.matches(self, config, runtime_workflow_config, None) {
                            return Some(commit)
                        },
                        None => {}
                    }
                }
                // if no matches for all for_targets of Some, find first for_targets of None
                for commit in v {
                    match commit.on {
                        // first only matches some for_targets and target
                        Some(_) => {},
                        None => return Some(commit)
                    }
                }
                return None;
            },
            // if no commits setting, return none
            None => return None
        }
    }
}

struct AggregatedPullRequestOptions {
    labels: Vec<config::Value>,
    assignees: Vec<config::Value>,
}
pub struct MatchOptions {
    check_condition: bool
}
impl MatchOptions {
    fn from(runtime_workflow_config: &config::runtime::Workflow) -> Self {
        Self {
            check_condition: runtime_workflow_config.job.is_none()
        }
    }
}

struct Node<'a> {
    job: Option<&'a Job>
}
type Graph<'a> = petgraph::Graph<Node<'a>, ()>;
type NodeId = petgraph::graph::NodeIndex<petgraph::graph::DefaultIx>;
pub struct DependencyGraph<'a>(Graph<'a>, HashMap<&'a String, NodeId>, NodeId);
impl<'a> DependencyGraph<'a> {
    pub fn new(jobs: &'a Jobs) -> Self {
        let mut dag = Graph::<'a>::new();
        let mut nodes = hashmap!{};
        let root = dag.add_node(Node::<'a>{job: None});
        let tail = dag.add_node(Node::<'a>{job: None});
        // first, register all jobs as dag node
        for (name, job) in jobs.as_map() {
            let n = dag.add_node(Node::<'a>{job: Some(job)});
            nodes.insert(name, n);
            dag.add_edge(nodes[name], root, ());
            dag.add_edge(tail, nodes[name], ());
        }
        // seconds, scan dependency settings and generate edge
        for (name, job) in jobs.as_map() {
            let n = nodes.get(name).expect(&format!("job '{}' does not exist", name));
            match &job.depends {
                Some(ds) => for d in ds {
                    let dn = nodes.get(&d.resolve()).expect(
                        &format!("dependent job '{}' does not exist", d)
                    );
                    dag.add_edge(*n, *dn, ());
                },
                None => {}
            };
        }
        Self(dag, nodes, tail)
    }
    pub fn traverse<F>(
        &self, start_job: Option<&str>, proc: F
    ) -> Result<(), Box<dyn Error>> where F: Fn(&'a str, &'a Job) -> Result<(), Box<dyn Error>> {
        // traversing dependency graph by dfs(post order), to run jobs ordered by dependency
        let mut visitor = petgraph::visit::DfsPostOrder::new(
            &self.0,
            start_job.map_or_else(
                || self.2,
                |name| *self.1.get(&name.to_string()).expect(&format!("job '{}' not found", name))
            )
        );
        while let Some(n) = visitor.next(&self.0) {
            match self.0[n].job {
                Some(j) => proc(&j.name, j)?,
                None => {}
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct Jobs(HashMap<String, Job>);
impl Jobs {
    pub fn setup(&mut self) {
        let map = &mut self.0;
        for (k, v) in map.iter_mut() {
            let name = &mut v.name;
            *name = k.to_string();
        }
    }
    pub fn as_map(&self) -> &HashMap<String, Job> {
        &self.0
    }
    pub fn find(&self, name: &str) -> Option<&Job> {
        self.as_map().get(name)
    }
    pub fn as_dg<'a>(
        &'a self
    ) -> DependencyGraph<'a> {
        DependencyGraph::<'a>::new(self)
    }
    pub fn run_steps(
        &self, config: &config::Config, shell: &impl shell::Shell, 
        runtime_workflow_config: &config::runtime::Workflow, job_name: &str, task: Option<&str>
    ) -> Result<Option<String>, Box<dyn Error>> {
        let job = self.as_map().get(job_name).expect(&format!("job {} does not exist", job_name));
        let runner = runner::Runner::new(job, config);
        runner.run_steps(shell, &shell::no_capture(), runtime_workflow_config,
            job, &runner.create_steps(&match task {
                Some(v) => panic!("steps for tasks ({}) are not supported yet", v),
                None => config::job::Command::Job
        }).0)
    }
    pub fn user_output(
        &self, config: &config::Config, job_name: &str, key: &str
    ) -> Result<Option<String>, Box<dyn Error>> {
        let job = self.as_map().get(job_name).expect(&format!("job {} does not exist", job_name));
        let ci = job.ci(config);
        match std::env::var("DEPLO_CI_JOB_NAME") {
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
    fn system_output(
        &self, config: &config::Config, job: &Job, key: &str
    ) -> Result<Option<String>, Box<dyn Error>> {
        let ci = job.ci(config);
        ci.job_output(&job.name, ci::OutputKind::System, key)
    }    
    pub fn set_user_output(
        &self, _: &config::Config, key: &str, value: &str
    ) -> Result<(), Box<dyn Error>> {
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
    fn push_job_result_branches(
        &self, config: &config::Config, branches_and_options: &(String, Vec<String>, CommitMethod)
    ) -> Result<(), Box<dyn Error>> {
        let vcs = config.modules.vcs();
        let job_id = std::env::var("DEPLO_CI_ID").unwrap();
        let current_branch = match std::env::var("DEPLO_CI_BRANCH_NAME") {
            Ok(v) => v,
            Err(_) => {
                log::debug!("current HEAD is not branch. skip push_job_result_branches");
                return Ok(())
            }
        };
        let current_ref = std::env::var("DEPLO_CI_CURRENT_COMMIT_ID").unwrap();
        let (name, branches, options) = branches_and_options;
        if branches.len() > 0 {
            let working_branch = format!("deplo-auto-commits-{}-{}", job_id, name);
            vcs.checkout(&current_ref, Some(&working_branch))?;
            for b in branches {
                vcs.fetch_branch(&b)?;
                // fetched head will be picked
                vcs.pick_fetched_head()?;
            }
            let result_head_ref = match options {
                CommitMethod::PullRequest{labels,assignees,..} => {
                    vcs.push_branch(&working_branch, &working_branch, &hashmap!{
                        "new" => "true",
                    })?;
                    let mut pr_opts_for_vcs = hashmap!{};
                    match labels {
                        Some(v) => { pr_opts_for_vcs.insert("labels", serde_json::to_string(&v)?); },
                        None => {}
                    };
                    match assignees {
                        Some(v) => { pr_opts_for_vcs.insert("assignees", serde_json::to_string(&v)?); },
                        None => {}
                    };
                    vcs.pr(
                        &format!("[deplo] auto commit by job [{}]", job_id),
                        &working_branch, &current_branch, 
                        &pr_opts_for_vcs.iter().map(|(k,v)| (*k,v.as_str())).collect()
                    )?;
                    // if pushed successfully, back to original branch
                    &current_ref
                },
                CommitMethod::Push{squash} => {
                    if squash.unwrap_or(true) && branches.len() > 1 {
                        vcs.squash_branch(branches.len())?;
                    }
                    vcs.push_branch(&working_branch, &current_branch, &hashmap!{})?;
                    // if pushed successfully, move current branch HEAD to pushed HEAD
                    &working_branch
                }
            };
            // only local execution need to recover repository status
            if !config::Config::is_running_on_ci() {
                vcs.checkout(result_head_ref, Some(&current_branch))?;
                vcs.delete_branch(vcs::RefType::Branch, &working_branch)?;
                for b in branches {
                    vcs.delete_branch(vcs::RefType::Remote, b)?;
                }
            }
        }
        Ok(())
    }    
    fn aggregate_commits(
        &self, config: &config::Config, runtime_workflow_config: &config::runtime::Workflow
    ) -> Result<Vec<(String, Vec<String>, CommitMethod)>, Box<dyn Error>> {
        let mut commits: Vec<(String, Vec<String>, CommitMethod)> = vec![];
        let mut aggregated_push_branches = vec![];
        let mut aggregated_pr_branches = vec![];
        let mut aggregated_pr_opts = AggregatedPullRequestOptions {
            labels: vec![], assignees: vec![],
        };
        for (job_name, job) in self.as_map() {
            match self.system_output(config, &job, DEPLO_SYSTEM_OUTPUT_COMMIT_BRANCH_NAME)? {
                Some(v) => {
                    log::info!("ci fin: find commit from job {} at {}", job_name, v);
                    match job.commit_setting_from_config(config, runtime_workflow_config)
                             .expect("commit_setting_from_release_target should success").method {
                        Some(ref options) => match options {
                            CommitMethod::Push{squash} => {
                                if squash.unwrap_or(true) {
                                    aggregated_push_branches.push(v);
                                } else {
                                    commits.push((format!("{}-push", job_name), vec![v], CommitMethod::Push{squash: Some(false)}));
                                }
                            },
                            CommitMethod::PullRequest{labels, assignees, aggregate} => {
                                if aggregate.unwrap_or(false) {
                                    aggregated_pr_branches.push(v);
                                    aggregated_pr_opts.labels = [aggregated_pr_opts.labels, labels.clone().unwrap_or(vec![])].concat();
                                    aggregated_pr_opts.assignees = [aggregated_pr_opts.assignees, assignees.clone().unwrap_or(vec![])].concat();
                                } else {
                                    commits.push((format!("{}-pr", job_name), vec![v], CommitMethod::PullRequest{
                                        labels: labels.as_ref().map(|v| v.clone()), assignees: assignees.as_ref().map(|v| v.clone()),
                                        aggregate: Some(false)
                                    }));
                                }
                            }
                        },
                        // if push option is not set, default behaviour is:
                        // push to current branch for integrate jobs,
                        // make pr to branch for deploy jobs.
                        None => if runtime_workflow_config.name == "integrate" {
                            // aggregated by default
                            aggregated_push_branches.push(v);
                        } else {
                            // made single PR by default
                            commits.push((format!("{}-pr", job_name), vec![v], CommitMethod::PullRequest{
                                labels: None, assignees: None, aggregate: Some(false)
                            }));
                        }
                    }
                },
                None => {}
            };
        }
        commits.push(("aggregate-push".to_string(), aggregated_push_branches, CommitMethod::Push{squash: Some(true)}));
        commits.push(("aggregate-pr".to_string(), aggregated_pr_branches, CommitMethod::PullRequest{
            labels: if aggregated_pr_opts.labels.len() > 0 { Some(aggregated_pr_opts.labels) } else { None }, 
            assignees: if aggregated_pr_opts.assignees.len() > 0 { Some(aggregated_pr_opts.assignees) } else { None }, 
            aggregate: Some(true)
        }));
        Ok(commits)
    }    
    pub fn halt(
        &self, config: &config::Config, runtime_workflow_config: &config::runtime::Workflow
    ) -> Result<(), Box<dyn Error>> {
        for branches_and_options in self.aggregate_commits(config, runtime_workflow_config)? {
            match self.push_job_result_branches(config, &branches_and_options) {
                Ok(_) => {},
                Err(e) => {
                    log::error!("push_job_result_branches fails: back to original branch");
                    let vcs = config.modules.vcs();
                    let current_branch = std::env::var("DEPLO_CI_BRANCH_NAME").unwrap();
                    let current_ref = std::env::var("DEPLO_CI_CURRENT_COMMIT_ID").unwrap();
                    vcs.checkout(&current_ref, Some(&current_branch)).unwrap();
                    return Err(e);
                }
            }
        }
        crate::util::try_debug!("deplo-halt", config.modules.ci_by_default(), runtime_workflow_config.exec, false);
        Ok(())
    }    
    fn wait_job(
        &self, job_id: &str, job_name: &str, config: &config::Config, runtime_workflow_config: &config::runtime::Workflow
    ) -> Result<(), Box<dyn Error>> {
        log::info!("wait for finishing remote job {} id={}", job_name, job_id);
        let job = match self.find(job_name) {
            Some(job) => job,
            None => return escalate!(Box::new(config::ConfigError{cause: format!("no such job: [{}]", job_name)})),
        };
        let ci = job.ci(config);
        let progress = !runtime_workflow_config.exec.silent;
        let mut timeout = runtime_workflow_config.exec.timeout;
        loop {
            match ci.check_job_finished(&job_id)? {
                Some(s) => if progress { 
                    print!(".{}", s);
                    std::io::stdout().flush().unwrap();
                },
                None => {
                    if progress {
                        println!(".done");
                    }
                    break
                },
            }
            sleep(Duration::from_secs(5));
            match timeout {
                Some(t) => if t > 5 {
                    timeout = Some(t - 5);
                } else {
                    return escalate!(Box::new(config::ConfigError{
                        cause: format!("remote job {} wait timeout {:?}", job_name, t)
                    }));
                },
                None => {}
            }
        }
        log::info!("remote job {} id={} finished", job_name, job_id);
        Ok(())
    }
    pub fn run(
        &self, config: &config::Config, runtime_workflow_config: &config::runtime::Workflow, shell: &impl shell::Shell
    ) -> Result<(), Box<dyn Error>> {
        let job_name = &runtime_workflow_config.job.as_ref().expect("should have job setting").name;
        if runtime_workflow_config.exec.follow_dependency {
            self.as_dg().traverse(Some(job_name), |name, job| {
                if !job.matches_current_trigger(config, runtime_workflow_config) {
                    log::debug!("run: job '{}' skipped because does not match trigger", name);
                    return Ok(())
                }
                match job.run(shell, config, runtime_workflow_config)? {
                    Some(job_id) => self.wait_job(&job_id, name, config, runtime_workflow_config)?,
                    None => {}
                };
                Ok(())
            })?;            
        } else {
            let job = config.jobs.find(job_name).expect(&format!("job '{}' does not exist", job_name));
            match job.run(shell, config, runtime_workflow_config)? {
                Some(job_id) => self.wait_job(&job_id, job_name, config, runtime_workflow_config)?,
                None => {}
            };
        }
        if !config::Config::is_running_on_ci() {
            log::debug!("if not running on CI, all jobs should be finished");
            self.halt(config, runtime_workflow_config)?;
        }
        Ok(())
    }
    pub fn boot(
        &self, config: &config::Config, runtime_workflow_config: &config::runtime::Workflow, shell: &impl shell::Shell
    ) -> Result<(), Box<dyn Error>> {
        let modules = &config.modules;
        let (_, ci) = modules.ci_by_env();
        // TODO: support follow_dependency of remote job running.
        // if runtime_workflow_config.job has some value and follow_dependency, 
        // we use Some(runtime_workflow_config.job.name) as first argument of traverse,
        // and modify runtime_workflow_config.job to None.
        // then all dependent jobs of runtime_workflow_config.job will be scheduled.
        // NOTE that we also need to change config::runtime::ExecOptions::apply implementation
        // so that follow_dependency option is respected for deplo boot too
        self.as_dg().traverse(None, |name, job| {
            if !job.matches_current_trigger(config, runtime_workflow_config) {
                log::debug!("boot: job '{}' skipped because does not match trigger", name);
                return Ok(())
            }
            if config::Config::is_running_on_ci() {
                ci.schedule_job(name)?;
            } else {
                match job.run(shell, config, runtime_workflow_config)? {
                    Some(job_id) => self.wait_job(&job_id, name, config, runtime_workflow_config)?,
                    None => {}
                };
            }
            Ok(())
        })?;
        crate::util::try_debug!("deplo-boot", ci, runtime_workflow_config.exec, false);
        if !config::Config::is_running_on_ci() {
            log::debug!("if not running on CI, all jobs should be finished");
            self.halt(config, runtime_workflow_config)?;
        }
        Ok(())
    }
}