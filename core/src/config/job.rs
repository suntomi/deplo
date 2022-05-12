use std::collections::{HashMap};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path};

use maplit::hashmap;
use serde::{Deserialize, Serialize};

use crate::ci;
use crate::config;
use crate::shell;
use crate::util::{escalate};
use crate::vcs;

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
pub enum FallbackContainer {
    /// docker image that is used for local execution.
    ImageUrl{ image: config::Value, shell: Option<config::Value> },
    /// dockerfile that is used for local execution. deplo build docker iamge with the dockerfile.
    DockerFile{ path: config::Value, repo_name: Option<config::Value>, shell: Option<config::Value> },
}
/// configuration of os of machine type runner
#[derive(Serialize, Deserialize, Clone, Copy, Eq, PartialEq)]
pub enum RunnerOS {
    #[serde(rename = "linux")]
    Linux,
    #[serde(rename = "windows")]
    Windows,
    #[serde(rename = "macos")]
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
/// configuration for runner of the job.
/// machine runner is use VM environment of CI service. less compatibility of local execution but faster.
/// container runner is use exactly same container for local execution as CI service. 
/// maximum compability of local execution but additional time to invoke job on CI service (for pulling image).
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
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
#[serde(tag = "type")]
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
    pub changed: Vec<config::Value>,
    pub on: Option<Trigger>,
    pub log_format: Option<config::Value>,
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
pub struct RunningOptions<'a> {
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
pub struct StepExtension {
    pub name: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum Step {
    Command {
        command: config::Value,
        name: Option<config::Value>,
        env: Option<HashMap<String, config::Value>>,
        shell: Option<config::Value>,
        workdir: Option<config::Value>,
    },
    Module(config::module::ConfigFor<crate::step::Module, StepExtension>)
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
            Step::Module(c) => c.value(|v| write!(f, 
                "Step::Module{{name:{}, uses:{}, with:{}", 
                c.ext().name.as_ref().map_or_else(|| "".to_string(), |s| s.to_string()),
                v.uses,
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
#[derive(Serialize, Deserialize)]
pub enum Trigger {
    Commit {
        workflows: Option<Vec<config::Value>>,
        release_targets: Option<Vec<config::Value>>,
        changed: Vec<config::Value>,
        diff_matcher: Option<config::Value>
    },
    Cron {
        schedule: config::Value
    },
    Repository {
        events: Vec<config::Value>
    },
    Module {
        workflow: Option<Vec<config::Value>>,
        when: HashMap<String, config::AnyValue>
    }
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
        config: &super::Config,
        runtime: &super::runtime::JobConfig
    ) -> bool {
        match &self {
            Self::Commit{ workflows, release_targets, changed, diff_matcher } => {
                if workflows.as_ref().map_or_else(
                    // when no workflow specified for condition, always pass
                    || false,
                    // if more than 1 workflows specified, runtime.workflow should match any of them
                    |v| v.iter().find(|v| { let s: &str = &v.resolve(); s == runtime.workflow }).is_none()
                ) {
                    return false;
                }
                if release_targets.as_ref().map_or_else(
                    // no job.release_targets restriction. always ok
                    || false,
                    |v| match &runtime.release_target {
                        // both runtime.release_target and job.release_targets exists, compare matches
                        Some(rt) => v.iter().find(|v| { let s: &str = &v.resolve(); s == rt }).is_none(),
                        // runtime.release_target exists, but job.release_targets is empty, always ok
                        None => false
                    }
                ) {
                    return false;
                }
                let dm = Self::diff_matcher(diff_matcher, changed);
                if !config.modules.vcs().changed(&dm) {
                    return false;
                }
            },
            Self::Cron{ schedule } => {
            },
            Self::Repository{ events } => {
            },
            Self::Module{ workflow, when } => {
            }
        }
        return true
    }
}
#[derive(Serialize, Deserialize)]
pub struct Job {
    pub account: Option<config::Value>,
    pub on: Vec<Trigger>,
    pub runner: Runner,
    pub shell: Option<config::Value>,
    pub command: Option<config::Value>,
    pub steps: Option<Vec<Step>>,
    pub env: Option<HashMap<String, config::Value>>,
    pub workdir: Option<config::Value>,
    pub checkout: Option<HashMap<String, config::Value>>,
    pub caches: Option<HashMap<String, Cache>>,
    pub depends: Option<Vec<config::Value>>,
    pub commits: Option<Vec<Commit>>,
    pub options: Option<HashMap<String, config::AnyValue>>,
    pub tasks: Option<HashMap<String, config::Value>>,
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
            Runner::Container{ .. } => false
        }
    }
    pub fn ci<'a>(&self, config: &'a super::Config) -> &Box<dyn ci::CI + 'a> {
        let account = self.account.as_ref().map_or_else(
            || "default".to_string(), config::Value::resolve_to_string
        );
        return config.modules.ci_for(&account);
    }
    pub fn job_env<'a>(&'a self, config: &'a super::Config, rtconfig: &super::runtime::JobConfig) -> HashMap<&'a str, String> {
        let ci = self.ci(config);
        let env = ci.job_env();
        let common_envs = hashmap!{
            "DEPLO_JOB_CURRENT_NAME" => rtconfig.job_name.clone()
        };
        match rtconfig.release_target {
            Some(ref v) => {
                log::info!("job_env: release target: {}", v);
            },
            None => {
                let (ref_type, ref_path) = config.modules.vcs().current_ref().unwrap();
                log::info!("job_env: no release target: {}/{}", ref_type, ref_path);
            }
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
    pub fn matches_current_trigger(
        &self,
        config: &super::Config,
        rtconfig: &super::runtime::JobConfig
    ) -> bool {
        for t in &self.on {
            if t.matches(config, rtconfig) {
                return true
            }
        }
        return false
    }
    pub fn commit_setting_from_config(
        &self,
        config: &super::Config,
        rtconfig: &super::runtime::JobConfig
    ) -> Option<&Commit> {
        match &self.commits {
            Some(ref v) => {
                for commit in v {
                    match commit.on {
                        // first only matches commit entry that has valid for_targets and target
                        Some(ref t) => if t.matches(config, rtconfig) {
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
    pub fn user_output(
        &self, config: &config::Config, job_name: &str, key: &str
    ) -> Result<Option<String>, Box<dyn Error>> {
        let ci = self.ci(config);
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
    pub fn system_output(
        &self, config: &config::Config, job_name: &str, key: &str
    ) -> Result<Option<String>, Box<dyn Error>> {
        let ci = self.ci(config);
        ci.job_output(job_name, ci::OutputKind::System, key)
    }    
}

/*
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

 */

#[derive(Serialize, Deserialize)]
pub struct Jobs(HashMap<String, Job>);
impl Jobs {
    pub fn as_map(&self) -> &HashMap<String, Job> {
        &self.0
    }
    pub fn run<S>(
        &self, job_name: &str, shell: &S, cmd: &Command, options: &RunningOptions
    ) -> Result<Option<String>, Box<dyn Error>> where S: shell::Shell {
        panic!("TODO: implement Jobs.run")
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
}