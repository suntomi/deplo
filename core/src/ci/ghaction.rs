use std::fs;
use std::fmt;
use std::error::Error;
use std::result::Result;
use std::collections::{HashMap};
use std::thread::sleep;
use std::time::Duration as StdDuration;

use chrono::{Utc, Duration};
use log;
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue};

use crate::config;
use crate::ci;
use crate::shell;
use crate::vcs;
use crate::util::{
    escalate,seal,
    MultilineFormatString,rm,
    maphash,
    sorted_key_iter,
    merge_hashmap,
    randombytes_as_string
};

#[derive(Deserialize)]
#[serde(untagged)]
enum EventPayload {
    Schedule {
        schedule: String
    },
    // enum variant 'RepositoryDispatch' should be defined earlier than 'Repository'
    // to avoid wrongly being matched as 'Repository' variant.
    RepositoryDispatch {
        action: String,
        client_payload: JsonValue
    },
    Repository {
        action: Option<String>
    }
}
impl fmt::Display for EventPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Schedule{schedule} => write!(f, "schedule:{}", schedule),
            Self::Repository{action} => {
                let a = if action.is_none() { "None" } else { action.as_ref().unwrap().as_str() };          
                write!(f, "repository:{}", a)
            },
            Self::RepositoryDispatch{action,..} => write!(f, "repository_dispatch:{}", action)
        }
    }
}
#[derive(Deserialize)]
struct WorkflowEvent {
    pub event_name: String,
    pub event: EventPayload
}
#[derive(Serialize, Deserialize)]
pub struct ClientPayload {
    pub job_id: String,
    #[serde(flatten)]
    pub job_config: config::runtime::Workflow
}
impl ClientPayload {
    fn new(
        job_config: &config::runtime::Workflow
    ) -> Self {
        Self {
            job_id: randombytes_as_string!(16),
            job_config: job_config.clone()
        }
    }
}

#[derive(Deserialize)]
pub struct PartialWorkflow {
    pub id: u64,
    pub status: String,
    pub url: String,
    pub jobs_url: String,
}
#[derive(Deserialize)]
pub struct PartialWorkflows {
    pub workflow_runs: Vec<PartialWorkflow>
}
#[derive(Deserialize)]
pub struct PartialJob {
    pub name: String
}
#[derive(Deserialize)]
pub struct PartialJobs {
    pub jobs: Vec<PartialJob>
}

#[derive(Deserialize)]
struct RepositoryPublicKeyResponse {
    pub key: String,
    pub key_id: String,
}

#[derive(Deserialize)]
struct RepositorySecret {
    pub name: String
}
#[derive(Deserialize)]
struct RepositorySecretsResponse {
    // total_count: u64,
    pub secrets: Vec<RepositorySecret>,
}

pub struct GhAction<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub account_name: String,
    pub shell: S,
}

impl<S: shell::Shell> GhAction<S> {
    fn job_output_env_name(kind: ci::OutputKind, job_name: &str) -> String {
        format!(
            "DEPLO_JOB_{}_OUTPUT_{}",
            kind.to_str().to_uppercase(), job_name.replace("-", "_").to_uppercase()
        )
    }
    fn generate_entrypoint<'a>(&self, config: &'a config::Config) -> Vec<String> {
        // get target branch
        let target_branches = sorted_key_iter(&config.release_targets)
            .filter(|v| v.1.is_branch())
            .map(|(_,v)| v.paths().iter().map(|p| p.resolve()).collect::<Vec<_>>())
            .collect::<Vec<_>>().concat();
        let target_tags = sorted_key_iter(&config.release_targets)
            .filter(|v| v.1.is_tag())
            .map(|(_,v)| v.paths().iter().map(|p| p.resolve()).collect::<Vec<_>>())
            .collect::<Vec<_>>().concat();
        let branches = if target_branches.len() > 0 { vec![format!("branches: [\"{}\"]", target_branches.join("\",\""))] } else { vec![] };
        let tags = if target_tags.len() > 0 { vec![format!("tags: [\"{}\"]", target_tags.join("\",\""))] } else { vec![] };
        format!(
            include_str!("../../res/ci/ghaction/entrypoint.yml.tmpl"), 
            branches = MultilineFormatString{
                strings: &branches,
                postfix: None
            },
            tags = MultilineFormatString{
                strings: &tags,
                postfix: None
            },
        ).split("\n").map(|s| s.to_string()).collect()
    }
    fn generate_outputs(&self, jobs: &HashMap<String, config::job::Job>) -> Vec<String> {
        sorted_key_iter(jobs).map(|(v,_)| {
            format!("{name}: ${{{{ steps.deplo-main.outputs.{name} }}}}", name = v)
        }).collect()
    }
    fn generate_need_cleanups<'a>(&self, jobs: &HashMap<String, config::job::Job>) -> String {
        sorted_key_iter(jobs).map(|(v,_)| {
            format!("needs.{name}.outputs.need-cleanup", name = v)
        }).collect::<Vec<String>>().join(" || ")
    }
    fn generate_cleanup_envs<'a>(&self, jobs: &HashMap<String, config::job::Job>) -> Vec<String> {
        let envs = sorted_key_iter(jobs).map(|(v,_)| {
            format!("{env_name}: ${{{{ needs.{job_name}.outputs.system }}}}",
                env_name = Self::job_output_env_name(ci::OutputKind::System, &v),
                job_name = v
            )
        }).collect::<Vec<String>>();
        if envs.len() > 0 {
            format!(include_str!("../../res/ci/ghaction/envs.yml.tmpl"),
                envs = MultilineFormatString{
                    strings: &envs,
                    postfix: None
                }
            ).split("\n").map(|s| s.to_string()).collect()
        } else {
            vec![]
        }
    }
    fn generate_debugger(&self, job: Option<&config::job::Job>, config: &config::Config) -> Vec<String> {
        let sudo = match job {
            Some(ref j) => {
                if !config.debug.as_ref().map_or_else(|| false, |v| v.get("ghaction_job_debugger").is_some()) &&
                    !j.options.as_ref().map_or_else(|| false, |v| v.get("debugger").is_some()) {
                    return vec![];
                }
                // if in container, sudo does not required to install debug instrument
                match j.runner {
                    config::job::Runner::Machine{..} => true,
                    config::job::Runner::Container{..} => false,        
                }
            },
            None => {
                // deplo kick/finish
                if !config.debug.as_ref().map_or_else(|| false, |v| v.get("ghaction_deplo_debugger").is_some()) {
                    return vec![]
                }
                true
            }
        };
        format!(
            include_str!("../../res/ci/ghaction/debugger.yml.tmpl"), 
            sudo = sudo
        ).split("\n").map(|s| s.to_string()).collect()        
    }
    fn generate_restore_keys(&self, cache: &config::job::Cache) -> Vec<String> {
        if cache.keys.len() > 1 {
            format!(
                include_str!("../../res/ci/ghaction/restore_keys.yml.tmpl"),
                keys = MultilineFormatString{
                    strings: &cache.keys[1..].iter().map(
                        config::Value::resolve_to_string
                    ).collect::<Vec<_>>(), postfix: None
                }
            ).split("\n").map(|s| s.to_string()).collect::<Vec<String>>()
        } else {
            vec![]
        }
    }
    fn generate_caches(&self, job: &config::job::Job) -> Vec<String> {
        match job.caches {
            Some(ref c) => sorted_key_iter(c).map(|(name,cache)| {
                format!(
                    include_str!("../../res/ci/ghaction/cache.yml.tmpl"), 
                    name = name, key = cache.keys[0], 
                    restore_keys = MultilineFormatString{
                        strings: &self.generate_restore_keys(&cache),
                        postfix: None
                    },
                    paths = MultilineFormatString{
                        strings: &cache.paths.iter().map(
                            config::Value::resolve_to_string
                        ).collect::<Vec<_>>(), postfix: None
                    },
                    env_key = format!("DEPLO_CACHE_{}_HIT", name.to_uppercase())
                ).split("\n").map(|s| s.to_string()).collect::<Vec<String>>()
            }).collect(),
            None => vec![]
        }.concat()
    }
    fn generate_command<'a>(&self, name: &str, job: &'a config::job::Job) -> Vec<String> {
        let cmd = format!("run: deplo run {}", name);
        match job.runner {
            config::job::Runner::Machine{os, ..} => {
                match os {
                    config::job::RunnerOS::Windows => vec![cmd, "shell: bash".to_string()],
                    _ => vec![cmd],
                }
            },
            config::job::Runner::Container{image:_} => vec![cmd],
        }
    }
    fn generate_job_envs<'a>(&self, job: &'a config::job::Job) -> Vec<String> {
        let lines = match job.depends {
            Some(ref depends) => {
                let mut envs = vec![];
                for d in depends {
                    envs.push(format!(
                        "{}: ${{{{ needs.{}.outputs.user }}}}",
                        Self::job_output_env_name(ci::OutputKind::User, &d.resolve()),
                        d
                    ));
                }
                envs
            },
            None => return vec![]
        };
        format!(include_str!("../../res/ci/ghaction/envs.yml.tmpl"),
            envs = MultilineFormatString{
                strings: &lines,
                postfix: None
            }
        ).split("\n").map(|s| s.to_string()).collect()
    }
    fn generate_job_dependencies<'a>(&self, depends: &'a Option<Vec<config::Value>>) -> String {
        depends.as_ref().map_or_else(
            || "deplo-main".to_string(),
            |v| {
                let mut vs = v.iter().map(
                    config::Value::resolve_to_string
                ).collect::<Vec<_>>();
                vs.push("deplo-main".to_string());
                format!("\"{}\"", vs.join("\",\""))
            })
    }
    fn generate_container_setting<'a>(&self, runner: &'a config::job::Runner) -> Vec<String> {
        match runner {
            config::job::Runner::Machine{ .. } => vec![],
            config::job::Runner::Container{ image } => vec![format!("container: {}", image)]
        }
    }
    fn generate_fetchcli_steps<'a>(&self, runner: &'a config::job::Runner) ->Vec<String> {
        let (path, uname, ext, shell) = match runner {
            config::job::Runner::Machine{ref os, ..} => match os {
                config::job::RunnerOS::Windows => ("/usr/bin/deplo", "Windows", ".exe", "shell: bash"),
                v => ("/usr/local/bin/deplo", v.uname(), "", "")
            },
            config::job::Runner::Container{image:_} => ("/usr/local/bin/deplo", "Linux", "", "")
        };
        let mut lines = format!(include_str!("../../res/ci/ghaction/fetchcli.yml.tmpl"),
            deplo_cli_path = path,
            download_url = format!(
                "{}/{}/deplo-{}{}",
                config::DEPLO_RELEASE_URL_BASE, config::DEPLO_VERSION, uname, ext
            )
        ).split("\n").map(|s| s.to_string()).collect::<Vec<String>>();
        if !shell.is_empty() {
            lines.push(format!("  {}", shell));
        }
        lines
    }
    fn generate_checkout_opts(&self, option_lines: &Vec<String>) -> Vec<String> {
        if option_lines.len() == 0 {
            return vec![];
        }
        format!(include_str!("../../res/ci/ghaction/with.yml.tmpl"),
            options = MultilineFormatString{
                strings: option_lines,
                postfix: None
            }
        ).split("\n").map(|s| s.to_string()).collect()
    }
    fn generate_checkout_steps<'a>(
        &self, _: &'a str, options: &'a Option<HashMap<String, config::Value>>, defaults: &Option<HashMap<String, config::Value>>
    ) -> Vec<String> {
        let merged_opts: HashMap<String, config::Value> = options.as_ref().map_or_else(
            || defaults.clone().unwrap_or(HashMap::new()),
            |v| merge_hashmap(v, &defaults.clone().unwrap_or(HashMap::new()))
        );
        let checkout_opts = sorted_key_iter(&merged_opts).map(|(k,v)| {
            return if vec!["fetch-depth", "lfs", "ref", "token"].contains(&k.as_str()) {
                format!("{}: {}", k, v)
            } else {
                format!("# warning: deplo only support lfs/fetch-depth options for github action checkout but {}({}) is specified", k, v)
            }
        }).collect::<Vec<String>>();
        // hash value for separating repository cache according to checkout options
        let opts_hash = options.as_ref().map_or_else(
            || "".to_string(), 
            |v| { format!("-{}", maphash(v)) }
        );
        if merged_opts.get("fetch-depth").map_or_else(|| false, |v| v.resolve().parse::<i32>().unwrap_or(-1) == 0) {
            format!(
                include_str!("../../res/ci/ghaction/cached_checkout.yml.tmpl"),
                checkout_opts = MultilineFormatString{
                    strings: &self.generate_checkout_opts(&checkout_opts),
                    postfix: None
                }, opts_hash = opts_hash
            ).split("\n").map(|s| s.to_string()).collect()
        } else {
            format!(
                include_str!("../../res/ci/ghaction/checkout.yml.tmpl"),
                checkout_opts = MultilineFormatString{
                    strings: &self.generate_checkout_opts(&checkout_opts),
                    postfix: None
                }
            ).split("\n").map(|s| s.to_string()).collect()
        }
    }
    fn get_token(&self) -> Result<config::Value, Box<dyn Error>> {
        let config = self.config.borrow();
        Ok(match config.ci.get(&self.account_name).unwrap() {
            config::ci::Account::GhAction { account, key, kind } => {
                let kind_resolved = match kind.as_ref() {
                    Some(v) => v.resolve(),
                    None => "user".to_string()
                };
                match kind_resolved.as_str() {
                    "user" => key.clone(),
                    "app" => return escalate!(Box::new(ci::CIError {
                        cause: format!(
                            "TODO: generate jwt with account {} as sub and encrypt with key",
                            account
                        ),
                    })),
                    v => return escalate!(Box::new(ci::CIError {
                        cause: format!("unsupported account kind {}", v),
                    })),
                }
            },
            _ => return escalate!(Box::new(ci::CIError {
                cause: "should have ghaction CI config but other config provided".to_string()
            }))
        })
    }
}

impl<S: shell::Shell> ci::CI for GhAction<S> {
    fn new(config: &config::Container, account_name: &str) -> Result<GhAction<S>, Box<dyn Error>> {
        return Ok(GhAction::<S> {
            config: config.clone(),
            account_name: account_name.to_string(),
            shell: S::new(config)
        });
    }
    fn runs_on_service(&self) -> bool {
        std::env::var("GITHUB_ACTION").is_ok()
    }
    fn generate_config(&self, reinit: bool) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let repository_root = config.modules.vcs().repository_root()?;
        // TODO_PATH: use Path to generate path of /.github/...
        let workflow_yml_path = format!("{}/.github/workflows/deplo-main.yml", repository_root);
        let create_main = config.ci.is_main("GhAction");
        fs::create_dir_all(&format!("{}/.github/workflows", repository_root))?;
        let previously_no_file = !rm(&workflow_yml_path);
        // inject secrets from dotenv file
        let mut secrets = vec!();
        for (k, v) in sorted_key_iter(&config::secret::vars()?) {
            if previously_no_file || reinit {
                (self as &dyn ci::CI).set_secret(k, v)?;
                log::debug!("set secret value of {}", k);
            }
            secrets.push(format!("{}: ${{{{ secrets.{} }}}}", k, k));
        }
        // generate job entries
        let mut job_descs = Vec::new();
        let mut all_job_names = vec!["deplo-main".to_string()];
        let mut lfs = false;
        let jobs = config.jobs.as_map();
        for (name, job) in sorted_key_iter(&jobs) {
            all_job_names.push(name.clone());
            let lines = format!(
                include_str!("../../res/ci/ghaction/job.yml.tmpl"), 
                name = name,
                needs = self.generate_job_dependencies(&job.depends),
                machine = match job.runner {
                    config::job::Runner::Machine{ref image, ref os, ..} => match image {
                        Some(v) => v.resolve(),
                        None => (match os {
                            config::job::RunnerOS::Linux => "ubuntu-latest",
                            config::job::RunnerOS::Windows => "windows-latest",
                            config::job::RunnerOS::MacOS => "macos-latest",
                        }).to_string()
                    },
                    config::job::Runner::Container{image:_} => "ubuntu-latest".to_string(),
                },
                caches = MultilineFormatString{
                    strings: &self.generate_caches(&job),
                    postfix: None
                },
                command = MultilineFormatString{
                    strings: &self.generate_command(name, &job),
                    postfix: None
                },
                job_envs = MultilineFormatString{
                    strings: &self.generate_job_envs(&job),
                    postfix: None
                },
                container = MultilineFormatString{
                    strings: &self.generate_container_setting(&job.runner),
                    postfix: None
                },
                fetchcli = MultilineFormatString{
                    strings: &self.generate_fetchcli_steps(&job.runner),
                    postfix: None
                },
                checkout = MultilineFormatString{
                    strings: &self.generate_checkout_steps(&name, &job.checkout, &Some(merge_hashmap(
                        &hashmap!{
                            "ref".to_string() => config::Value::new("${{ github.event.client_payload.exec.revision }}"),
                            "fetch-depth".to_string() => config::Value::new("2")
                        }, &job.checkout.as_ref().map_or_else(
                            || if job.commit.is_some() {
                                hashmap!{ "fetch-depth".to_string() => config::Value::new("0") }
                            } else {
                                hashmap!{}
                            },
                            |v| if v.get("lfs").is_some() || job.commit.is_some() {
                                hashmap!{ "fetch-depth".to_string() => config::Value::new("0") }
                            } else {
                                hashmap!{}
                            }
                        )))
                    ),
                    postfix: None
                },
                debugger = MultilineFormatString{
                    strings: &self.generate_debugger(Some(&job), &config),
                    postfix: None
                }
            ).split("\n").map(|s| s.to_string()).collect::<Vec<String>>();
            job_descs = job_descs.into_iter().chain(lines.into_iter()).collect();
            // check if lfs is enabled
            if !lfs {
                lfs = job.checkout.as_ref().map_or_else(|| false, |v| v.get("lfs").is_some() && job.commit.is_some());
                if lfs {
                    log::debug!("there is an job {}, that has commits option and does lfs checkout. enable lfs for deplo fin", name);
                }
            }
        }
        fs::write(&workflow_yml_path,
            format!(
                include_str!("../../res/ci/ghaction/main.yml.tmpl"), 
                remote_job_dispatch_type = config::DEPLO_REMOTE_JOB_EVENT_TYPE,
                entrypoint = MultilineFormatString{ 
                    strings: &(if create_main { self.generate_entrypoint(&config) } else { vec![] }),
                    postfix: None
                },
                secrets = MultilineFormatString{ strings: &secrets, postfix: None },
                outputs = MultilineFormatString{ 
                    strings: &self.generate_outputs(jobs),
                    postfix: None
                },
                fetchcli = MultilineFormatString{
                    strings: &self.generate_fetchcli_steps(&config::job::Runner::Machine{
                        os: config::job::RunnerOS::Linux, image: None, class: None, local_fallback: None }
                    ),
                    postfix: None
                },
                kick_checkout = MultilineFormatString{
                    strings: &self.generate_checkout_steps("main", &None, &Some(hashmap!{
                        "fetch-depth".to_string() => config::Value::new("2"),
                        "ref".to_string() => config::Value::new("${{ github.event.client_payload.commit }}")
                    })),
                    postfix: None
                },
                fin_checkout = MultilineFormatString{
                    strings: &self.generate_checkout_steps("main", &None, &Some(hashmap!{
                        "fetch-depth".to_string() => config::Value::new("2"),
                        "ref".to_string() => config::Value::new("${{ github.event.client_payload.commit }}"),
                        "lfs".to_string() => config::Value::new(&lfs.to_string())
                    })),
                    postfix: None
                },
                jobs = MultilineFormatString{
                    strings: &job_descs,
                    postfix: None
                },
                debugger = MultilineFormatString{
                    strings: &self.generate_debugger(None, &config),
                    postfix: None
                },
                need_cleanups = &self.generate_need_cleanups(jobs),
                cleanup_envs = MultilineFormatString{
                    strings: &self.generate_cleanup_envs(jobs),
                    postfix: None
                },
                needs = format!("\"{}\"", all_job_names.join("\",\""))
            )
        )?;
        Ok(())
    }
    fn overwrite_commit(&self, commit: &str) -> Result<String, Box<dyn Error>> {
        let prev = std::env::var("GITHUB_SHA")?;
        std::env::set_var("GITHUB_SHA", commit);
        Ok(prev)
    }
    fn pr_url_from_env(&self) -> Result<Option<String>, Box<dyn Error>> {
        match std::env::var("DEPLO_GHACTION_PR_URL") {
            Ok(v) => if v.is_empty() { Ok(None) } else { Ok(Some(v)) },
            Err(e) => {
                match e {
                    std::env::VarError::NotPresent => Ok(None),
                    _ => return escalate!(Box::new(e))
                }
            }
        }
    }
    fn schedule_job(&self, job_name: &str) -> Result<(), Box<dyn Error>> {
        println!("::set-output name={}::true", job_name);
        Ok(())
    }
    fn mark_need_cleanup(&self, job_name: &str) -> Result<(), Box<dyn Error>> {
        if config::Config::is_running_on_ci() {
            println!("::set-output name=need-cleanup::true");
        } else {
            log::debug!("mark_need_cleanup: {}", job_name);
        }
        Ok(())
    }
    fn filter_workflows(
        &self, trigger: Option<ci::WorkflowTrigger>
    ) -> Result<Vec<config::runtime::Workflow>, Box<dyn Error>> {
        let resolved_trigger = match trigger {
            Some(t) => t,
            // on github action, full event payload is stored env var 'DEPLO_GHACTION_EVENT_DATA' 
            None => match std::env::var("DEPLO_GHACTION_EVENT_DATA") {
                Ok(v) => ci::WorkflowTrigger::EventPayload(v),
                Err(_) => panic!(
                    "DEPLO_GHACTION_EVENT_DATA should set if on github acton or no argument for workflow (-w) passed"
                )
            }
        };
        let config = self.config.borrow();
        match resolved_trigger {
            ci::WorkflowTrigger::EventPayload(payload) => {
                let mut matched_names = vec![];
                let workflow_event = serde_json::from_str::<WorkflowEvent>(&payload)?;
                for (k,v) in config.workflows.as_map() {
                    let name = k.to_string();
                    match workflow_event.event_name.as_str() {
                        "push" => match v {
                            config::workflow::Workflow::Deploy => matched_names.push(name),
                            _ => {}
                        },
                        "pull_request" => match v {
                            config::workflow::Workflow::Integrate => matched_names.push(name),
                            _ => {}
                        },
                        "schedule" => match v {
                            config::workflow::Workflow::Cron{..} => matched_names.push(name),
                            _ => {}
                        },
                        // repository_dispatch has a few possibility.
                        // config::DEPLO_REMOTE_JOB_EVENT_TYPE => should contain workflow name in client_payload["name"]
                        // config::DEPLO_MODULE_EVENT_TYPE => Module workflow invocation
                        // others => Repository workflow invocation
                        "repository_dispatch" => if let EventPayload::RepositoryDispatch{
                            action, client_payload
                        } = &workflow_event.event {
                            if action == config::DEPLO_REMOTE_JOB_EVENT_TYPE {
                                match &client_payload["name"] {
                                    JsonValue::String(s) => {
                                        let workflow_name = s.to_string();
                                        if workflow_name == name {
                                            return Ok(vec![config::runtime::Workflow::with_payload(
                                                &serde_json::to_string(client_payload)?
                                            )?]);
                                        }
                                    },
                                    _ => panic!(
                                        "{}: event payload invalid {}", 
                                        config::DEPLO_REMOTE_JOB_EVENT_TYPE, client_payload
                                    )
                                }
                            } else if action == config::DEPLO_MODULE_EVENT_TYPE {
                                if let config::workflow::Workflow::Module(..) = v {
                                    log::warn!("TODO: should check current workflow is matched for the module?");
                                    matched_names.push(name);
                                }
                            } else if let config::workflow::Workflow::Repository{..} = v {
                                matched_names.push(name);
                            }
                        } else {
                            panic!("event payload type does not match {}", workflow_event.event);
                        },
                        _ => match v {
                            config::workflow::Workflow::Repository{..} => matched_names.push(name),
                            _ => {}
                        }
                    }
                }
                let mut matches = vec![];
                let vcs = config.modules.vcs();
                for n in matched_names {
                    let name = n.to_string();
                    match config.workflows.get(&name).expect(&format!("workflow {} not found", name)) {
                        config::workflow::Workflow::Deploy|config::workflow::Workflow::Integrate => {
                            let target = vcs.release_target();
                            matches.push(config::runtime::Workflow::with_context(
                                name, if target.is_none() { hashmap!{} } else { hashmap!{
                                    "release_target".to_string() => config::AnyValue::new(&target.unwrap())
                                }}
                            ))
                        },
                        config::workflow::Workflow::Cron{schedules} => {
                            if let EventPayload::Schedule{ref schedule} = workflow_event.event {
                                match schedules.iter().find_map(|(k, v)| {
                                    if v.resolve().as_str() == schedule.as_str() { Some(k) } else { None }
                                }) {
                                    Some(schedule_name) => matches.push(config::runtime::Workflow::with_context(
                                        name, hashmap!{ "schedule".to_string() => config::AnyValue::new(schedule_name) }
                                    )),
                                    None => {}
                                }
                            } else {
                                panic!("event payload type does not match {}", workflow_event.event);
                            }
                        },
                        config::workflow::Workflow::Repository{events,..} => {
                            if let EventPayload::Repository{ref action} = workflow_event.event {
                                let key = {
                                    let mut v = vec![workflow_event.event_name.as_str()];
                                    if let Some(action) = action {
                                        v.push(action.as_str())
                                    }
                                    v.join(".")
                                };
                                match events.iter().find_map(|(k, vs)| {
                                    if vs.iter().find(|t| t == &key).is_some() { Some(k) } else { None }
                                }) {
                                    Some(event_name) => matches.push(config::runtime::Workflow::with_context(
                                        name, hashmap!{ "event".to_string() => config::AnyValue::new(event_name) }
                                    )),
                                    None => {}
                                }
                            } else {
                                panic!("event payload type does not match {}", workflow_event.event);
                            }
                        },
                        config::workflow::Workflow::Module(_c) => {
                            panic!("not implemented yet")
                        }
                    }
                }
                Ok(matches)
            }
        }
    }
    fn run_job(&self, job_config: &config::runtime::Workflow) -> Result<String, Box<dyn Error>> {
        let config = self.config.borrow();
        let token = self.get_token()?;
        let user_and_repo = config.modules.vcs().user_and_repo()?;
        let payload = ClientPayload::new(job_config);
        self.shell.exec(shell::args![
            "curl", "-H", shell::fmtargs!("Authorization: token {}", &token), 
            "-H", "Accept: application/vnd.github.v3+json", 
            format!(
                "https://api.github.com/repos/{}/{}/dispatches", 
                user_and_repo.0, user_and_repo.1
            ),
            "-d", format!(r#"{{
                "event_type": "{job_name}",
                "client_payload": {client_payload}
            }}"#, 
                job_name = config::DEPLO_REMOTE_JOB_EVENT_TYPE,
                client_payload = serde_json::to_string(&payload)?
            )
        ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
        log::debug!("wait for remote job to start.");
        let mut count = 0;
        // we have 1 minutes buffer to search for created workflow
        // to be tolerant of clock skew
        let start = Utc::now().checked_sub_signed(Duration::minutes(1)).unwrap();
        loop {
            let response = self.shell.exec(shell::args![
                "curl", "-H", shell::fmtargs!("Authorization: token {}", &token), 
                "-H", "Accept: application/vnd.github.v3+json", 
                format!(
                    "https://api.github.com/repos/{}/{}/actions/runs?event=repository_dispatch&created={}",
                    user_and_repo.0, user_and_repo.1,
                    format!(">{}", start.to_rfc3339())
                )
            ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
            log::trace!("current workflows by remote execution: {}", response);
            let workflows = serde_json::from_str::<PartialWorkflows>(&response)?;
            if workflows.workflow_runs.len() > 0 {
                for wf in workflows.workflow_runs {
                    let response = self.shell.exec(shell::args![
                        "curl", "-H", shell::fmtargs!("Authorization: token {}", &token), 
                        "-H", "Accept: application/vnd.github.v3+json", wf.jobs_url
                    ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
                    let parsed = serde_json::from_str::<PartialJobs>(&response)?;
                    if parsed.jobs.len() > 0 && parsed.jobs[0].name.contains(&payload.job_id) {
                        log::info!("remote job started at: {}", wf.url);
                        return Ok(wf.id.to_string());
                    }
                }
            }
            count = count + 1;
            if count > 12 {
                return escalate!(Box::new(ci::CIError {
                    cause: format!("timeout waiting for remote job to start {}", payload.job_id),
                }));
            }
            sleep(StdDuration::from_secs(1));
            if config.runtime.verbosity > 0 {
                print!(".");
            }
        }
    }
    fn check_job_finished(&self, job_id: &str) -> Result<Option<String>, Box<dyn Error>> {
        let config = self.config.borrow();
        let token = self.get_token()?;
        let user_and_repo = config.modules.vcs().user_and_repo()?;
        let response = self.shell.exec(shell::args![
            "curl", "-H", shell::fmtargs!("Authorization: token {}", &token),
            "-H", "Accept: application/vnd.github.v3+json",
            format!(
                "https://api.github.com/repos/{}/{}/actions/runs/{}",
                user_and_repo.0, user_and_repo.1, job_id
            )
        ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
        let parsed = serde_json::from_str::<PartialWorkflow>(&response)?;
        if parsed.status == "completed" {
            return Ok(None);
        }
        return Ok(Some(parsed.status));
    }
    fn job_output(&self, job_name: &str, kind: ci::OutputKind, key: &str) -> Result<Option<String>, Box<dyn Error>> {
        match std::env::var(&Self::job_output_env_name(kind, job_name)) {
            Ok(value) => {
                if value.is_empty() {
                    return Ok(None);
                }
                match serde_json::from_str::<HashMap<String, String>>(&value)?.get(key) {
                    Some(value) => Ok(Some(value.to_string())),
                    None => Ok(None),
                }
            },
            Err(_) => Ok(None)
        }
    }
    fn set_job_output(&self, job_name: &str, kind: ci::OutputKind, outputs: HashMap<&str, &str>) -> Result<(), Box<dyn Error>> {
        let text = serde_json::to_string(&outputs)?;
        if config::Config::is_running_on_ci() {
            println!("::set-output name={}::{}", kind.to_str(), text);
        } else {
            std::env::set_var(&Self::job_output_env_name(kind, job_name), &text);
        }
        Ok(())
    }
    fn process_env(&self) -> Result<HashMap<&str, String>, Box<dyn Error>> {
        let config = self.config.borrow();
        let vcs = config.modules.vcs();
        let mut envs = hashmap!{
            "DEPLO_CI_TYPE" => "GhAction".to_string()
        };
        // get from env
        for (src, target) in hashmap!{
            "DEPLO_CI_ID" => "DEPLO_GHACTION_CI_ID",
            "DEPLO_CI_PULL_REQUEST_URL" => "DEPLO_GHACTION_PR_URL"
        } {
            match std::env::var(src) {
                Ok(v) => {
                    envs.insert(target, v);
                },
                Err(_) => {}
            }
        };
        match std::env::var("GITHUB_SHA") {
            Ok(v) => envs.insert(
                "DEPLO_CI_CURRENT_COMMIT_ID", 
                match vcs.current_ref()? {
                    (vcs::RefType::Pull, _) => {
                        let output = vcs.commit_hash(Some(&format!("{}^@", v)))?;
                        let commits = output.split("\n").collect::<Vec<&str>>();
                        if commits.len() > 1 {
                            commits[1].to_string()
                        } else {
                            commits[0].to_string()
                        }
                    },
                    (_, _) => v
                }
            ),
            Err(_) => None
        };
        match std::env::var("GITHUB_HEAD_REF") {
            Ok(v) if !v.is_empty() => {
                envs.insert("DEPLO_CI_BRANCH_NAME", v);
            },
            _ => match std::env::var("GITHUB_REF_TYPE") {
                Ok(ref_type) => {
                    match std::env::var("GITHUB_REF") {
                        Ok(ref_name) => {
                            match ref_type.as_str() {
                                "branch" => envs.insert(
                                    "DEPLO_CI_BRANCH_NAME", ref_name.replace("refs/heads/", "")
                                ),
                                "tag" => envs.insert(
                                    "DEPLO_CI_TAG_NAME", ref_name.replace("refs/tags/", "")
                                ),
                                v => { log::error!("invalid ref_type {}", v); None },
                            };
                        },
                        Err(_) => { log::error!("GITHUB_REF_TYPE is set but GITHUB_REF is not set"); },
                    }
                },
                Err(_) => {}
            }
        };
        Ok(envs)
    }
    fn job_env(&self) -> HashMap<String, config::Value> {
        hashmap!{}
    }
    fn list_secret_name(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let token = self.get_token()?;
        let config = self.config.borrow();
        let user_and_repo = config.modules.vcs().user_and_repo()?;
        let response = match serde_json::from_str::<RepositorySecretsResponse>(
            &self.shell.exec(shell::args![
            "curl", format!(
                "https://api.github.com/repos/{}/{}/actions/secrets",
                user_and_repo.0, user_and_repo.1
            ),
            "-H", shell::fmtargs!("Authorization: token {}", &token),
            "-H", "Accept: application/json"
        ], shell::no_env(), shell::no_cwd(), &shell::capture())?) {
            Ok(v) => v,
            Err(e) => return escalate!(Box::new(e))
        };
        Ok(
            response.secrets.iter().map(|s| s.name.clone()).collect()
        )
    }
    fn set_secret(&self, key: &str, value: &str) -> Result<(), Box<dyn Error>> {
        let token = self.get_token()?;
        let config = self.config.borrow();
        let user_and_repo = config.modules.vcs().user_and_repo()?;
        let pkey_response = &self.shell.exec(shell::args![
                "curl", format!(
                    "https://api.github.com/repos/{}/{}/actions/secrets/public-key",
                    user_and_repo.0, user_and_repo.1
                ),
                "-H", shell::fmtargs!("Authorization: token {}", &token)
            ], shell::no_env(), shell::no_cwd(), &shell::capture()
        )?;
        let public_key_info = match serde_json::from_str::<RepositoryPublicKeyResponse>(pkey_response) {
            Ok(v) => v,
            Err(e) => {
                log::error!("fail to get public key to encode secret: {}", pkey_response);
                return escalate!(Box::new(e))
            }
        };
        let json = format!("{{\"encrypted_value\":\"{}\",\"key_id\":\"{}\"}}", 
            //get value from env to unescapse
            seal(value, &public_key_info.key)?,
            public_key_info.key_id
        );
        // TODO_PATH: use Path to generate path of /dev/null
        let status = self.shell.exec(shell::args!(
            "curl", "-X", "PUT",
            format!(
                "https://api.github.com/repos/{}/{}/actions/secrets/{}",
                user_and_repo.0, user_and_repo.1, key
            ),
            "-H", "Content-Type: application/json",
            "-H", "Accept: application/json",
            "-H", shell::fmtargs!("Authorization: token {}", &token),
            "-d", json, "-w", "%{http_code}", "-o", "/dev/null"
        ), shell::no_env(), shell::no_cwd(), &shell::capture())?.parse::<u32>()?;
        if status >= 200 && status < 300 {
            Ok(())
        } else {
            return escalate!(Box::new(ci::CIError {
                cause: format!("fail to set secret to CircleCI CI with status code:{}", status)
            }));
        }
    }
}