use std::collections::{HashMap};
use std::fmt;

use maplit::hashmap;
use serde::{Deserialize, Serialize};

use crate::config;
use crate::shell;
use crate::vcs;

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
    pub targets: Option<Vec<config::Value>>,
    pub log_format: Option<config::Value>,
    pub method: Option<CommitMethod>,
}
impl Commit {
    fn generate_commit_log(&self, name: &str, _: &Job) -> String {
        //TODO: pass variable to log_format
        self.log_format.as_ref().unwrap_or(
            &format!("[deplo] update by job {job_name}", job_name = name)
        ).to_string()
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
    Module(config::module::ConfigFor<crate::step::Manifest>)
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
            Step::Module(c) => write!(f, 
                "Step::Module{{uses:{}, with:{}", 
                //name.as_ref().map_or_else(|| "".to_string(), |s| s.to_string()),
                c.value().uses,
                c.value().with.as_ref().map_or_else(|| "None".to_string(), |e| format!("{:?}", e))
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
pub enum Trigger {
    Commit {
        workflows: Option<Vec<config::Value>>,
        targets: Option<Vec<config::Value>>,
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
        workflows: Option<Vec<config::Value>>,
        when: HashMap<String, config::AnyValue>
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
            Runner::Container{ image: _ } => false
        }
    }
    pub fn job_env<'a>(&'a self, config: &'a super::Config, system: &SystemJobEnvOptions) -> HashMap<&'a str, String> {
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