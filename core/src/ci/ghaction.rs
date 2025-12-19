use std::env;
use std::fs;
use std::fmt;
use std::error::Error;
use std::result::Result;
use std::collections::HashMap;
use std::thread::sleep;
use std::time::Duration as StdDuration;
use std::vec;

use chrono::{Utc, Duration};
use log;
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::config;
use crate::config::value;
use crate::ci::{self, CheckoutOption};
use crate::shell;
use crate::vcs;
use crate::util::{
    escalate,seal,
    MultilineFormatString,rm,
    sorted_key_iter,
    merge_hashmap,
    randombytes_as_string,
    escape
};
use crate::vcs::github::AppTokenGenerator;

lazy_static! {
    pub static ref DEPLO_GHACTION_MODULE_VERSIONS: HashMap<String, String> = hashmap! {
        "actions/checkout".to_string() => "v5".to_string(),
        "actions/cache".to_string() => "v5".to_string(),
        "mxschmitt/action-tmate".to_string() => "c0afd6f790e3a5564914980036ebf83216678101".to_string(),
    };
}

fn get_module_version(module: &str) -> String {
    // generate environment variable name from module name
    // eg. actions/checkout => DEPLO_GHACTION_MODULE_VERSION_ACTIONS_CHECKOUT
    let env_name = format!("DEPLO_GHACTION_MODULE_VERSION_{}",
        module.to_uppercase().replace("/", "_").replace("-", "_")
    );
    if env::var(&env_name).is_ok() {
        match env::var(&env_name) {
            Ok(v) => return v,
            Err(_) => {}
        }
    }
    DEPLO_GHACTION_MODULE_VERSIONS.get(module).unwrap().clone()
}

#[derive(Serialize, Deserialize)]
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
    WorkflowDispatch {
        inputs: JsonValue
    },
    Repository {
        action: Option<String>
    }
}
#[derive(Deserialize)]
struct WebIdentityTokenResponse {
    // count: u64,
    value: String
}
impl fmt::Display for EventPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Schedule{schedule} => write!(f, "schedule:{}", schedule),
            Self::Repository{action} => {
                let a = if action.is_none() { "None" } else { action.as_ref().unwrap().as_str() };          
                write!(f, "repository:{}", a)
            },
            Self::WorkflowDispatch{inputs} => {
                write!(f, "workflow_dispatch:{}", inputs)
            }
            Self::RepositoryDispatch{action,..} => write!(f, "repository_dispatch:{}", action)
        }
    }
}
#[derive(Deserialize)]
struct WorkflowEvent {
    pub event_name: String,
    pub event: EventPayload
}
#[derive(Deserialize)]
struct RefSpec {
    #[serde(rename = "ref")]
    pub refspec: String,
    pub sha: String
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

impl ci::CheckoutOption for config::job::CheckoutOption {
    fn to_yaml_config(value: &config::Value) -> String {
        if value.is_secret() {
            format!("${{{{ secrets.{} }}}}", value.raw_value())
        } else {
            value.resolve()
        }
    }
    fn opt_str(&self, account: &config::ci::Account) -> Vec<String> {
        let mut r = vec![];
        match self.fetch_depth {
            Some(ref v) => r.push(format!("fetch-depth: {}", v)),
            None => {}
        } 
        match self.lfs {
            Some(ref v) => r.push(format!("lfs: {}", v)),
            None => {}
        }
        match self.submodules {
            Some(ref v) => match v {
                config::job::SubmoduleCheckoutType::Checkout(b) => if *b {
                    r.push(format!("submodules: {}", b))
                },
                config::job::SubmoduleCheckoutType::Recursive => {
                    r.push("submodules: recursive".to_string())
                }
            },
            None => {}
        }
        match account {
            config::ci::Account::GhAction{..} => match self.token {
                Some(ref v) => r.push(format!("token: {}", Self::to_yaml_config(v))),
                None => {}
            },
            config::ci::Account::GhActionApp{..} => {
                // use github app token generated in previous step
                r.push("token: ${{ steps.app-token.outputs.token }}".to_string());
            },
            _ => {}
        }
        return r
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
    pub app_token_generator: Option<AppTokenGenerator<S>>
}

lazy_static! {
    static ref G_ALL_SECRET_TARGETS: Vec<String> = {
        vec!["actions".to_string(), "dependabot".to_string()]
    };
}
impl<S: shell::Shell> GhAction<S> {
    fn set_secret_base(&self, key: &str, value: &str, path: &str) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let user_and_repo = config.modules.vcs().user_and_repo()?;
        let (token, auth_type) = self.get_token()?;
        let pkey_response = &self.shell.exec(shell::args![
                "curl", format!(
                    "https://api.github.com/repos/{}/{}/{}/secrets/public-key",
                    user_and_repo.0, user_and_repo.1, path
                ),
                "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token)
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
                "https://api.github.com/repos/{}/{}/{}/secrets/{}",
                user_and_repo.0, user_and_repo.1, path, key
            ),
            "-H", "Content-Type: application/json",
            "-H", "Accept: application/json",
            "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token),
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

impl<S: shell::Shell> GhAction<S> {
    fn generate_manual_dispatch(&self, schemas: &config::workflow::InputSchemaSet) -> Vec<String> {
        let mut input_configs = vec![];
        for (name, schema) in sorted_key_iter(schemas.as_map()) {
            let mut options = vec![];
            if schema.description.is_some() {
                options.push(format!("description: '{}'", schema.description.as_ref().unwrap()));
            }
            if schema.required.is_some() {
                options.push(format!("required: {}", schema.required.unwrap()));
            }
            if schema.default.is_some() {
                options.push(format!("default: {}", schema.default.as_ref().unwrap().to_string()));
            }
            match &schema.class {
                config::workflow::InputSchemaClass::Value{ty,..} => match ty {
                    config::workflow::InputValueType::Bool => options.push(format!("type: boolean")),
                    config::workflow::InputValueType::Number => options.push(format!("type: number")),
                    config::workflow::InputValueType::Float => options.push(format!("type: number")),
                    _ => {},
                },
                config::workflow::InputSchemaClass::Enum{options: opts} => {
                    options.push(format!("type: choice"));
                    for line in format!(
                        include_str!("../../res/ci/ghaction/key_and_values.yml.tmpl"),
                        key = "options",
                        values = MultilineFormatString{
                            strings: &opts.iter().map(|o| format!("- {}", o)).collect::<Vec<String>>(),
                            postfix: None
                        }
                    ).split("\n").map(|s| s.to_string()).collect::<Vec<String>>() {
                        options.push(line);
                    }
                },
                _ => {}
            }
            input_configs.push(format!(
                include_str!("../../res/ci/ghaction/key_and_values.yml.tmpl"),
                key = name,
                values = MultilineFormatString{
                    strings: &options,
                    postfix: None
                }
            ).split("\n").map(|s| s.to_string()).collect::<Vec<String>>());
        }
        input_configs.concat()
    }
    fn generate_entrypoints<'a>(&self, config: &'a config::Config) -> HashMap<String, Vec<String>> {
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
        // get other repository events
        let mut event_entries = hashmap!{};
        let mut workflow_dispatch_entries = hashmap!{};
        let mut repository_dispatches = vec![
            config::DEPLO_MODULE_EVENT_TYPE.to_string()
        ];
        let mut schedule_entries = vec![];
        for (name, v) in sorted_key_iter(config.workflows.as_map()) {
            match v {
                config::workflow::Workflow::Repository { events, .. } => {
                    for (_, event_names) in events {
                        for event_name in event_names {
                            let name = event_name.resolve();
                            let components: Vec<&'_ str> = name.split(".").collect();
                            match components.len() {
                                1 => { event_entries.entry(components[0].to_string()).or_insert(vec![]); },
                                2 => {
                                    event_entries.
                                        entry(components[0].to_string()).
                                        or_insert(vec![]).
                                        push(components[1].to_string())
                                },
                                _ => panic!("invalid event specifier '{}', should be form of 'category.event'", event_name)
                            }
                        }
                    }
                },
                config::workflow::Workflow::Cron { schedules, .. } => {
                    schedule_entries.push(sorted_key_iter(schedules).map(|(_,v)| { format!("- cron: {}", v) }).collect::<Vec<_>>())
                },
                config::workflow::Workflow::Deploy|config::workflow::Workflow::Integrate => {},
                config::workflow::Workflow::Dispatch{inputs,manual, ..} => {
                    if manual.unwrap_or(false) {
                        if name == "main" {
                            panic!("deplo does not allow manual dispatch name 'main'")
                        }
                        workflow_dispatch_entries.entry(name).or_insert(self.generate_manual_dispatch(inputs));
                    } else if name != config::DEPLO_SYSTEM_WORKFLOW_NAME {
                        repository_dispatches.push(name.replace("_", "-"));
                    }
                },
                config::workflow::Workflow::Module(_) => {}
            }
        }
        let schedules = if schedule_entries.len() > 0 { 
            format!(
                include_str!("../../res/ci/ghaction/key_and_values.yml.tmpl"),
                key = "schedule",
                values = MultilineFormatString{
                    strings: &schedule_entries.concat(),
                    postfix: None
                }
            ).split("\n").map(|s| s.to_string()).collect::<Vec<String>>()
        } else {
            vec![]
        };
        let events = sorted_key_iter(&event_entries).map(|(k,v)| {
            if v.len() <= 0 {
                vec![format!("{}:", k)]
            } else {
                format!("{}:\n{:>2}", 
                    k, MultilineFormatString{
                        strings: &vec![format!("types: [{}]", v.join(","))],
                        postfix: None
                    }
                ).split("\n").map(|s| s.to_string()).collect::<Vec<String>>()
            }
        }).collect::<Vec<Vec<String>>>().concat();
        let mut results = hashmap!{};
        results.insert("main".to_string(), format!(
            include_str!("../../res/ci/ghaction/entrypoint.yml.tmpl"), 
            branches = MultilineFormatString{
                strings: &branches,
                postfix: None
            },
            tags = MultilineFormatString{
                strings: &tags,
                postfix: None
            },
            schedules = MultilineFormatString{
                strings: &schedules,
                postfix: None
            },
            repository_dispatches = repository_dispatches.join(","),
            events = MultilineFormatString{
                strings: &events,
                postfix: None
            }
        ).split("\n").map(|s| s.to_string()).collect());
        let mut keys: Vec<_> = config.jobs.as_map().keys().collect();
        keys.sort();
        results.insert("system".to_string(), format!(
            include_str!("../../res/ci/ghaction/system_entrypoint.yml.tmpl"), 
            jobs = MultilineFormatString{
                strings: &keys.iter().map(|v| format!("- {}", v)).collect::<Vec<String>>(),
                postfix: None
            },
        ).split("\n").map(|s| s.to_string()).collect());
        for (dispatch_name, lines) in workflow_dispatch_entries {
            results.insert(dispatch_name.replace("_", "-"), format!(
                include_str!("../../res/ci/ghaction/workflow_entrypoint.yml.tmpl"), 
                inputs = MultilineFormatString{
                    strings: &lines,
                    postfix: None
                }
            ).split("\n").map(|s| s.to_string()).collect());
        }
        results
    }
    fn generate_outputs(&self, jobs: &HashMap<&String, &config::job::Job>) -> Vec<String> {
        sorted_key_iter(jobs).map(|(v,_)| {
            format!("{name}: ${{{{ steps.deplo-main.outputs.{name} }}}}", name = v)
        }).collect()
    }
    fn generate_halt_exec_conditions<'a>(&self, jobs: &HashMap<&String, &config::job::Job>) -> String {
        sorted_key_iter(jobs).map(|(v,_)| {
            format!("needs.{name}.outputs.need-cleanup", name = v)
        }).collect::<Vec<String>>().join(" || ")
    }
    fn generate_failure_conditions<'a>(&self, jobs: &HashMap<&String, &config::job::Job>) -> String {
        sorted_key_iter(jobs).map(|(v,_)| {
            format!("needs.{name}.result == 'failure'", name = v)
        }).collect::<Vec<String>>().join(" || ")
    }
    fn generate_cleanup_envs<'a>(&self, jobs: &HashMap<&String, &config::job::Job>) -> Vec<String> {
        let envs = sorted_key_iter(jobs).map(|(v,_)| {
            format!("{env_name}: ${{{{ needs.{job_name}.outputs.system }}}}",
                env_name = ci::OutputKind::System.env_name_for_job(&v),
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
        let (sudo, always) = match job {
            Some(ref j) => {
                let global_config = config.debug.as_ref().map_or_else(|| None, |v| v.get("ghaction_job_debugger"));
                let job_config = j.options.as_ref().map_or_else(|| None, |v| v.get("debugger"));
                let always = match job_config.map(|v| v.as_str()).unwrap_or(Some("default")) {
                    Some("none") => return vec![],
                    Some("always") => true,
                    _ => match global_config.map(|v| v.as_str()).unwrap_or("default") {
                        "none" => return vec![],
                        "always" => true,
                        _ => false
                    }
                };
                // if in container, sudo does not required to install debug instrument
                (match j.runner {
                    config::job::Runner::Machine{..} => true,
                    config::job::Runner::Container{..} => false,
                }, always)
            },
            None => {
                let global_config = config.debug.as_ref().map_or_else(|| None, |v| v.get("ghaction_deplo_debugger"));
                // deplo boot/halt
                (true, match global_config.map(|v| v.as_str()).unwrap_or("default") {
                    "none" => return vec![],
                    "always" => true,
                    _ => false
                })
            }
        };
        format!(
            include_str!("../../res/ci/ghaction/debugger.yml.tmpl"), 
            sudo = sudo,
            condition = if always {
                ""
            } else {
                "if: always() && (env.DEPLO_CI_RUN_DEBUGGER != '')"
            },
            debugger_version = get_module_version("mxschmitt/action-tmate")
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
            config::job::Runner::Container{..} => vec![cmd],
        }
    }
    fn generate_job_envs<'a>(&self, job: &'a config::job::Job) -> Vec<String> {
        let lines = match job.depends {
            Some(ref depends) => {
                let mut envs = vec![];
                for d in depends {
                    envs.push(format!(
                        "{}: ${{{{ needs.{}.outputs.user }}}}",
                        ci::OutputKind::User.env_name_for_job(&d.resolve()),
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
    fn generate_native_configs<'a>(&self, job: &config::job::Job) -> Vec<String> {
        match job.options {
            Some(ref o) => match o.get("native_configs") {
                Some(v) => if v.is_table() {
                    v.as_yaml().split("\n").map(|s| s.to_string()).collect::<Vec<String>>()
                } else {
                    log::warn!("native_configs option should be a table. ignore it");
                    vec![]
                }
                None => vec![]
            },
            None => vec![]
        }
    }
    fn generate_container_setting<'a>(&self, runner: &'a config::job::Runner) -> Vec<String> {
        match runner {
            config::job::Runner::Machine{ .. } => vec![],
            config::job::Runner::Container{ image, .. } => vec![format!("container: {}", image)]
        }
    }
    fn generate_fetchcli_steps<'a>(&self, runner: &'a config::job::Runner) ->Vec<String> {
        let (path, uname, ext, shell) = match runner {
            config::job::Runner::Machine{ref os, ..} => match os {
                config::job::RunnerOS::Windows => ("/usr/bin/deplo", "Windows", ".exe", "shell: bash"),
                config::job::RunnerOS::Linux => ("/usr/local/bin/deplo", "Linux-$(uname -m)", "", ""),
                v => ("/usr/local/bin/deplo", v.uname(), "", "")
            },
            config::job::Runner::Container{..} => ("/usr/local/bin/deplo", "Linux", "", "")
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
    fn generate_cache_restore_cmds(&self, options: &config::job::CheckoutOption) -> String {
        let mut cmds = vec!["git reset --hard HEAD"];
        if options.submodules.as_ref().map_or_else(|| false, |v| match v {
            config::job::SubmoduleCheckoutType::Checkout(b) => *b,
            config::job::SubmoduleCheckoutType::Recursive => true
        }) {
            cmds.push("git submodule update --init --recursive");
            cmds.push("deplo -v=1 ci restore-cache --submodules");
        } else {
            cmds.push("deplo -v=1 ci restore-cache");
        }
        cmds.join(" && ")
    }
    fn generate_checkout_steps<'a>(
        &self, _: &'a str, account: &config::ci::Account, options: &'a Option<config::job::CheckoutOption>, defaults: &Option<config::job::CheckoutOption>
    ) -> Vec<String> {
        let merged_opts = match options.as_ref() {
            Some(v) => match defaults.as_ref() {
                Some(vv) => vv.merge(v),
                None => v.clone()
            },
            None => match defaults.as_ref() {
                Some(v) => v.clone(),
                None => config::job::CheckoutOption::default()
            }
        };
        let checkout_opts = merged_opts.opt_str(account);
        // hash value for separating repository cache according to checkout options
        let opts_hash = options.as_ref().map_or_else(
            || "".to_string(), 
            |v| { format!("-{}", v.hash(account)) }
        );
        let mut steps = match account {
            config::ci::Account::GhAction{..} => { vec![] },
            config::ci::Account::GhActionApp{app_id_secret_name, pkey_secret_name, ..} => {
                format!(
                    include_str!("../../res/ci/ghaction/app_token.yml.tmpl"),
                    app_id_secret_name = app_id_secret_name.resolve(),
                    pkey_secret_name = pkey_secret_name.resolve()
                ).split("\n").map(|s| s.to_string()).collect::<Vec<String>>()
            },
            _ => panic!("generate_checkout_steps should be called with GhAction account")
        };
        if merged_opts.fetch_depth.map_or_else(|| false, |v| v == 0) {
            let mut lines = format!(
                include_str!("../../res/ci/ghaction/cached_checkout.yml.tmpl"),
                checkout_opts = MultilineFormatString{
                    strings: &self.generate_checkout_opts(&checkout_opts),
                    postfix: None
                }, opts_hash = opts_hash, restore_commands = &self.generate_cache_restore_cmds(&merged_opts),
                checkout_version = get_module_version("actions/checkout"),
                cache_version = get_module_version("actions/cache")
            ).split("\n").map(|s| s.to_string()).collect();
            steps.append(&mut lines);
        } else {
            let mut lines = format!(
                include_str!("../../res/ci/ghaction/checkout.yml.tmpl"),
                checkout_opts = MultilineFormatString{
                    strings: &self.generate_checkout_opts(&checkout_opts),
                    postfix: None
                },
                checkout_version = get_module_version("actions/checkout")
            ).split("\n").map(|s| s.to_string()).collect();
            steps.append(&mut lines);
        }
        steps
    }
    fn get_token(&self) -> Result<(config::Value, &str), Box<dyn Error>> {
        let config = self.config.borrow();
        Ok(match config.ci.get(&self.account_name).unwrap() {
            config::ci::Account::GhAction { key, .. } => (key.clone(), "token"),
            config::ci::Account::GhActionApp { local_fallback, .. } => {
                // Use local_fallback when running locally and fallback is configured
                if !config::Config::is_running_on_ci() {
                    if let Some(fallback) = local_fallback {
                        return Ok((fallback.key.clone(), "token"));
                    }
                }
                (config::Value::new_sensitive(&self.app_token_generator.as_ref().unwrap().generate()?), "Bearer")
            },
            _ => return escalate!(Box::new(ci::CIError {
                cause: "should have ghaction CI config but other config provided".to_string()
            }))
        })
    }
    fn set_output(&self, key: &str, val: &str) -> Result<(), Box<dyn Error>> {
        let base64_val = base64::encode(val.as_bytes());
        // log::warn!("set output {}", format!("echo \'{key}={base64_val}\' >> $GITHUB_OUTPUT", key = key));
        self.shell.eval(
            &format!("echo \'{key}={base64_val}\' >> $GITHUB_OUTPUT", key = key, base64_val = base64_val),
            &None, hashmap!{"GITHUB_OUTPUT" => shell::arg!(std::env::var("GITHUB_OUTPUT")?)},
            shell::no_cwd(), &shell::no_capture()
        )?;
        Ok(())
    }
    fn set_env(&self, key: &str, val: &str) -> Result<(), Box<dyn Error>> {
        self.shell.eval(
            &format!("echo \'{key}={val}\' >> $GITHUB_ENV", key = key, val = val),
            &None, hashmap!{"GITHUB_ENV" => shell::arg!(std::env::var("GITHUB_ENV")?)},
            shell::no_cwd(), &shell::no_capture()
        )?;
        Ok(())
    }
}

impl<S: shell::Shell> ci::CI for GhAction<S> {
    fn new(config: &config::Container, account_name: &str) -> Result<GhAction<S>, Box<dyn Error>> {
        return Ok(GhAction::<S> {
            config: config.clone(),
            account_name: account_name.to_string(),
            shell: S::new(config),
            app_token_generator: match config.borrow().ci.get(account_name).expect(&format!("account {} not configured", account_name)) {
                config::ci::Account::GhAction{..} => None,
                config::ci::Account::GhActionApp{
                    ref app_id_secret_name, ref pkey_secret_name, ref local_fallback
                } => {
                    // Use local_fallback when running locally and fallback is configured
                    if !config::Config::is_running_on_ci() && local_fallback.is_some() {
                        None
                    } else {
                        let app_id = config::Value::new_secret(app_id_secret_name.resolve().as_str());
                        let pkey = config::Value::new_secret(pkey_secret_name.resolve().as_str());
                        Some(AppTokenGenerator::<S>::new(
                            S::new(config), app_id, pkey,
                            config.borrow().modules.vcs.as_ref().unwrap().user_and_repo()?
                        ))
                    }
                },
                _ => panic!("DEPLO_GHACTION_EVENT_DATA should set")
            }
        });
    }
    fn runs_on_service(&self) -> bool {
        std::env::var("GITHUB_ACTION").is_ok()
    }
    fn restore_cache(&self, submodule: bool) -> Result<(), Box<dyn Error>> {
        log::info!("restore cache: start submodule = {}", submodule);
        match std::env::var("DEPLO_GHACTION_EVENT_DATA") {
            Ok(v) => {
                log::info!("restore cache: current HEAD => {}", self.shell.output_of(
                    shell::args!["git", "describe", "--all"], shell::no_env(), shell::no_cwd()
                )?);
                // setup repository to ensure match with current ref
                // when using cache, sometimes repository is not match with current ref
                // eg. main repository cached and make tag with same commit of main HEAD, workflow invoked by the tag
                // has same commit but wrong ref (refs/heads/main instead of, say, refs/tags/v0.1.0)                
                let rs: RefSpec = serde_json::from_str(&v)?;
                log::info!("refspec: {}:{}", rs.sha, rs.refspec);
                let config = self.config.borrow();
                let vcs = config.modules.vcs();
                let (refspec, branch) = match &rs.refspec {
                    v if v.starts_with("refs/heads/") => {
                        log::info!("restore heads ref");
                        (format!("refs/remotes/origin/{}", &rs.refspec[11..]), Some(&rs.refspec[11..]))
                    },
                    v if v.starts_with("refs/tags/") => {
                        log::info!("restore tags ref");
                        (v.clone(), None)
                    },
                    v if v.starts_with("refs/pull/") => {
                        log::info!("restore pulls ref");
                        (format!("refs/remotes/{}", &rs.refspec[5..]), None)
                    },
                    _ => panic!("invalid refspec {}", rs.refspec)
                };
                log::info!("restore with: sha={} ref={}", rs.sha, refspec);
                vcs.fetch_object(&rs.sha, &refspec, None)?;
                vcs.checkout(&refspec, branch)?;
                if submodule {
                    // init submodule
                    log::info!("restore submodule");
                    self.shell.exec(
                        shell::args!["git", "submodule", "update", "--init", "--recursive"],
                        shell::no_env(), shell::no_cwd(), &shell::capture()
                    )?;
                }
            },
            Err(_) => panic!("DEPLO_GHACTION_EVENT_DATA should set")
        }
        Ok(())
    }
    fn generate_config(&self, reinit: bool) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let account = config.ci.get(&self.account_name).expect(&format!(
            "CI account {} should exist", self.account_name
        ));
        let jobs = config.jobs.as_map().iter().filter(
            |(_,v)| v.is_enabled_for_account(&self.account_name)
        ).collect::<HashMap<_,_>>();
        if jobs.len() == 0 {
            log::info!(
                "no jobs defined for the account {}. skip ghaction config generation", 
                self.account_name);
            return Ok(());
        }
        let config_post_fix = match self.account_name.as_str() {
            "default" => "".to_string(),
            _ => format!("-{}", self.account_name)
        };
        let repository_root = config.modules.vcs().repository_root()?;
        // TODO_PATH: use Path to generate path of /.github/...
        let main_workflow_yml_path = format!(
            "{}/.github/workflows/deplo-main{}.yml", repository_root, config_post_fix);
        let create_main = config.ci.is_main(vec!["GhAction", "GhActionApp"]);
        fs::create_dir_all(&format!("{}/.github/workflows", repository_root))?;
        let previously_no_file = !rm(&main_workflow_yml_path);
        // inject secrets from dotenv file
        let mut secrets = vec!();
        for (k, v) in sorted_key_iter(&config::secret::vars()?) {
            if previously_no_file || reinit {
                let targets = config::secret::targets(k.as_str());
                (self as &dyn ci::CI).set_secret(k, v, &targets)?;
                log::debug!("set secret value of {}", k);
            }
            secrets.push(format!("{}: ${{{{ secrets.{} }}}}", k, k));
        }
        for (k, v) in sorted_key_iter(&config::var::vars()?) {
            if previously_no_file || reinit {
                (self as &dyn ci::CI).set_var(k, v)?;
                log::debug!("set variable value of {}", k);
            }
            secrets.push(format!("{}: ${{{{ vars.{} }}}}", k, k));
        }
        // generate job entries
        let mut job_descs = Vec::new();
        let mut all_job_names = vec!["deplo-main".to_string()];
        let mut lfs = false;
        for (name, job) in sorted_key_iter(&jobs) {
            all_job_names.push(name.to_string());
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
                    config::job::Runner::Container{..} => "ubuntu-latest".to_string(),
                },
                native_configs = MultilineFormatString{
                    strings: &self.generate_native_configs(&job),
                    postfix: None
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
                    strings: &self.generate_checkout_steps(&name, account, &match config.checkout.as_ref() {
                        Some(v) => match job.checkout.as_ref() {
                            Some(vv) => Some(v.merge(vv)),
                            None => Some(v.clone())
                        },
                        None => job.checkout.clone()
                    }, &Some(config::job::CheckoutOption {
                            fetch_depth: Some(2),
                            lfs: None, token: None, submodules: None
                        }.merge(
                            &job.checkout.as_ref().map_or_else(
                                || config::job::CheckoutOption::default(),
                                // for lfs, set fetch_depth to 0 to pull all commits and using cache
                                |v| if v.lfs.unwrap_or(false) {
                                    config::job::CheckoutOption {
                                        fetch_depth: Some(0), lfs: None, token: None, submodules: None
                                    }
                                } else {
                                    config::job::CheckoutOption::default()
                                }
                            )
                        )
                    )),
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
                lfs = job.checkout.as_ref().map_or_else(|| false, |v| v.lfs.unwrap_or(false) && job.commit.is_some());
                if lfs {
                    log::debug!("there is an job {}, that has commits option and does lfs checkout. enable lfs for deplo fin", name);
                }
            }
        }
        let entrypoints = self.generate_entrypoints(&config);
        for (name, entrypoint) in entrypoints {
            let workflow_yml_path = format!("{}/.github/workflows/deplo-{}{}.yml", repository_root, name, config_post_fix);
            fs::write(&workflow_yml_path,
                format!(
                    include_str!("../../res/ci/ghaction/main.yml.tmpl"), 
                    workflow_name = match name.as_str() {
                        "main" => "Deplo Workflow Runner",
                        "system" => "Deplo System",
                        _ => &name
                    },
                    entrypoint = MultilineFormatString{
                        strings: &(if create_main { entrypoint } else { vec![] }),
                        postfix: None
                    },
                    secrets = MultilineFormatString{ strings: &secrets, postfix: None },
                    outputs = MultilineFormatString{ 
                        strings: &self.generate_outputs(&jobs),
                        postfix: None
                    },
                    fetchcli = MultilineFormatString{
                        strings: &self.generate_fetchcli_steps(&config::job::Runner::Machine{
                            os: config::job::RunnerOS::Linux, image: None, class: None, local_fallback: None, no_fallback: None }
                        ),
                        postfix: None
                    },
                    boot_checkout = MultilineFormatString{
                        strings: &self.generate_checkout_steps("main", account, &config.checkout, &Some(config::job::CheckoutOption {
                            fetch_depth: Some(2), lfs: None, token: None, submodules: None,
                        })),
                        postfix: None
                    },
                    halt_checkout = MultilineFormatString{
                        strings: &self.generate_checkout_steps("main", account, &config.checkout, &Some(config::job::CheckoutOption {
                            fetch_depth: Some(2), lfs: Some(lfs), token: None, submodules: None,
                        })),
                        postfix: None
                    },
                    halt_exec_condition = self.generate_halt_exec_conditions(&jobs),
                    failure_condition = self.generate_failure_conditions(&jobs),
                    jobs = MultilineFormatString{
                        strings: &job_descs,
                        postfix: None
                    },
                    debugger = MultilineFormatString{
                        strings: &self.generate_debugger(None, &config),
                        postfix: None
                    },
                    cleanup_envs = MultilineFormatString{
                        strings: &self.generate_cleanup_envs(&jobs),
                        postfix: None
                    },
                    needs = format!("\"{}\"", all_job_names.join("\",\""))
                )
            )?;
        }
        Ok(())
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
        self.set_output(job_name, "true")
    }
    fn mark_need_cleanup(&self, job_name: &str) -> Result<(), Box<dyn Error>> {
        if config::Config::is_running_on_ci() {
            self.set_output("need-cleanup", "true")?;
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
        match &resolved_trigger {
            ci::WorkflowTrigger::EventPayload(payload) => {
                let mut matched_names = vec![];
                let workflow_event = serde_json::from_str::<WorkflowEvent>(payload)?;
                for (k,v) in config.workflows.as_map() {
                    let name = k.to_string();
                    // module type workflow always matched here,
                    // then filtered when generating matches from matched_names
                    match v {
                        config::workflow::Workflow::Module(_) => {
                            matched_names.push(name);
                            continue;
                        },
                        _ => {}
                    }
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
                        // repository_dispatch has multiple possibility.
                        // config::DEPLO_MODULE_EVENT_TYPE => Module workflow invocation
                        // others => Repository workflow invocation
                        "repository_dispatch" => if let EventPayload::RepositoryDispatch{
                            action, ..
                        } = &workflow_event.event {
                            if action == config::DEPLO_MODULE_EVENT_TYPE {
                                if let config::workflow::Workflow::Module(..) = v {
                                    log::warn!("TODO: should check current workflow is matched for the module?");
                                    matched_names.push(name);
                                }
                            } else if let config::workflow::Workflow::Dispatch{..} = v {
                                if *action == name {
                                    matched_names.push(name);
                                } else {
                                    log::debug!("repository dispatch name does not match {} != {}", action, name)
                                }
                            }
                        } else {
                            panic!("event payload type does not match {}", workflow_event.event);
                        },
                        "workflow_dispatch" => if let config::workflow::Workflow::Dispatch{..} = v {
                            let dispatch_name = std::env::var("DEPLO_GHACTION_WORKFLOW_NAME").expect(
                                &format!("DEPLO_GHACTION_WORKFLOW_NAME should set")
                            );
                            if dispatch_name == config::DEPLO_SYSTEM_WORKFLOW_NAME && name == config::DEPLO_SYSTEM_WORKFLOW_NAME {
                                matched_names.push(config::DEPLO_SYSTEM_WORKFLOW_NAME.to_string());
                            } else if name.replace("_", "-") == dispatch_name {
                                matched_names.push(name);
                            } else {
                                log::debug!("workflow dispatch name does not match {} != {}", dispatch_name, name)
                            }
                        },
                        _ => match v {
                            config::workflow::Workflow::Repository{..} => matched_names.push(name),
                            _ => {}
                        }
                    }
                }
                let mut matches = vec![];
                for name in matched_names {
                    match config.workflows.get(&name).expect(&format!("workflow {} not found", name)) {
                        config::workflow::Workflow::Deploy|config::workflow::Workflow::Integrate => {
                            matches.push(config::runtime::Workflow::with_context(
                                name, hashmap!{}
                            ))
                        },
                        config::workflow::Workflow::Cron{schedules, ..} => {
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
                                let matched_events = events.iter()
                                    .filter(|(_,vs)| vs.iter().find(|t| t == &key).is_some())
                                    .map(|(k,_)| k.as_str()).collect::<Vec<&str>>();
                                if matched_events.len() > 0 {
                                    matches.push(config::runtime::Workflow::with_context(
                                        name, hashmap!{ 
                                            "events".to_string() => config::AnyValue::new_from_vec(&matched_events) 
                                        }
                                    ))
                                }
                            } else {
                                panic!("event payload type does not match {}", workflow_event.event);
                            }
                        },
                        config::workflow::Workflow::Dispatch{inputs,manual, ..} => {
                            match &workflow_event.event {
                                EventPayload::RepositoryDispatch{client_payload,action} => if !manual.unwrap_or(false) {
                                    inputs.verify(&client_payload); // panic!s when schema does not matched
                                    matches.push(config::runtime::Workflow::with_context(
                                        action.to_string(), serde_json::from_str(&serde_json::to_string(&client_payload)?)?
                                    ));
                                },
                                EventPayload::WorkflowDispatch{inputs: client_payload} => if name == config::DEPLO_SYSTEM_WORKFLOW_NAME {
                                    matches.push(config::runtime::Workflow::with_system_dispatch(
                                        // input format collectness is checked by this deserialize
                                        &serde_json::from_str(&serde_json::to_string(&client_payload)?)?
                                    ));
                                } else if manual.unwrap_or(false) {
                                    inputs.verify(&client_payload); // panic!s when schema does not matched
                                    matches.push(config::runtime::Workflow::with_context(
                                        name, serde_json::from_str(&serde_json::to_string(&client_payload)?)?
                                    ));
                                },
                                _ => { panic!("event payload type does not match {}", workflow_event.event); }
                            }
                        },
                        config::workflow::Workflow::Module(c) => {
                            if let Some(event_payload) = c.value(|v| {
                                let event = match &resolved_trigger {
                                    ci::WorkflowTrigger::EventPayload(payload) => payload,
                                };
                                log::debug!("check module workflow [{}] matches with setting {:?}", 
                                    v.uses.to_string(), v.with);
                                config.modules.workflow(&v.uses).filter_event(event, &v.with)
                            })? {
                                matches.push(config::runtime::Workflow::with_context(
                                    name, serde_json::from_str(&event_payload)?
                                ));
                            } else {
                                log::debug!("module workflow '{}' does not match with event payload", name);
                            }
                        }
                    }
                }
                Ok(matches)
            }
        }
    }
    fn run_job(&self, job_config: &config::runtime::Workflow) -> Result<String, Box<dyn Error>> {
        let config = self.config.borrow();
        let user_and_repo = config.modules.vcs().user_and_repo()?;
        let (token, auth_type) = self.get_token()?;
        let payload = ClientPayload::new(job_config);
        let mut inputs = hashmap!{
            "id" => payload.job_id.clone(),
            "workflow" => job_config.name.clone(),
            "context" => serde_json::to_string(&job_config.context)?,
            "exec" => serde_json::to_string(&job_config.exec)?,
            "job" => job_config.job.as_ref().unwrap().name.clone(),
        };
        match job_config.job {
            Some(ref j) => match j.command { 
                Some(ref c) => match c.args {
                    Some(ref a) => { inputs.insert("command", a.join(" ")); },
                    None => {}
                },
                None => {} 
            },
            None => {}
        }
        let commit = match &job_config.exec.revision {
            Some(v) => v.clone(),
            None => config.modules.vcs().commit_hash(None)?
        };
        let response = self.shell.exec(shell::args![
            "curl", "-f", "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token),
            "-H", "Accept: application/vnd.github.v3+json", 
            format!(
                "https://api.github.com/repos/{}/{}/actions/workflows/{}/dispatches", 
                user_and_repo.0, user_and_repo.1, config::DEPLO_SYSTEM_WORKFLOW_ID,
            ),
            "-d", format!(r#"{{
                "ref": "{remote_ref}",
                "inputs": {inputs}
            }}"#, 
                remote_ref = if commit.starts_with("refs") {
                    commit
                } else {
                    match config.modules.vcs().search_remote_ref(&commit)? {
                        Some(v) => v,
                        None => return escalate!(Box::new(ci::CIError {
                            cause: format!("remote ref for {} not found", commit),
                        }))
                    }
                },
                inputs = serde_json::to_string(&inputs)?
            )
        ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
        log::trace!("response: [{}]", response);
        log::debug!("wait for remote job to start.");
        let mut count = 0;
        // we have 1 minutes buffer to search for created workflow
        // to be tolerant of clock skew
        let start = Utc::now().checked_sub_signed(Duration::minutes(1)).unwrap();
        loop {
            let response = self.shell.exec(shell::args![
                "curl", "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token), 
                "-H", "Accept: application/vnd.github.v3+json", 
                format!(
                    "https://api.github.com/repos/{}/{}/actions/runs?event=workflow_dispatch&created={}",
                    user_and_repo.0, user_and_repo.1,
                    format!(">{}", start.to_rfc3339())
                )
            ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
            log::trace!("current workflows by remote execution: {}", response);
            let workflows = serde_json::from_str::<PartialWorkflows>(&response)?;
            if workflows.workflow_runs.len() > 0 {
                for wf in workflows.workflow_runs {
                    let response = self.shell.exec(shell::args![
                        "curl", "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token), 
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
        let user_and_repo = config.modules.vcs().user_and_repo()?;
        let (token, auth_type) = self.get_token()?;
        let response = self.shell.exec(shell::args![
            "curl", "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token),
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
        match std::env::var(&kind.env_name_for_job(job_name)) {
            Ok(value) => {
                log::debug!("job_output: got env {}={}", &kind.env_name_for_job(job_name), value);
                if value.is_empty() {
                    return Ok(None);
                }
                let decoded = match base64::decode(value.to_string()) {
                    Ok(decoded) => match String::from_utf8(decoded) {
                        Ok(v) => {
                            log::debug!("job_output: decoded {}={}, key={}", &kind.env_name_for_job(job_name), v, key);
                            v
                        },
                        Err(e) => return escalate!(Box::new(ci::CIError {
                            cause: format!("output value[{}] is not utf8 string: {:?}", value, e),
                        }))
                    },
                    Err(e) => return escalate!(Box::new(ci::CIError {
                        cause: format!("output value[{}] is not valid base64 string: {:?}", value, e),
                    }))
                };
                match serde_json::from_str::<HashMap<String, String>>(&decoded)?.get(key) {
                    Some(v) => Ok(Some(v.to_string())),
                    None => Ok(None),
                }
            },
            Err(e) => {
                log::debug!("job_output: fail to got env {} {:?}", &kind.env_name_for_job(job_name), e);
                Ok(None)
            }
        }
    }
    fn set_job_output(&self, job_name: &str, kind: ci::OutputKind, outputs: HashMap<&str, &str>) -> Result<(), Box<dyn Error>> {
        let text = serde_json::to_string(&outputs)?;
        if config::Config::is_running_on_ci() {
            self.set_output(kind.to_str(), &text)?;
        } else {
            let base64_text = base64::encode(text.as_bytes());
            std::env::set_var(&kind.env_name_for_job(job_name), &base64_text);
        }
        Ok(())
    }
    fn set_job_env(&self, envs: HashMap<&str, &str>) -> Result<(), Box<dyn Error>> {
        if config::Config::is_running_on_ci() {
            for (k, v) in envs {
                self.set_env(k, v)?;
            }
        } else {
            for (k, v) in envs {
                std::env::set_var(k, v);
            }
        }
        Ok(())
    }
    fn process_env(&self) -> Result<HashMap<&str, String>, Box<dyn Error>> {
        let config = self.config.borrow();
        let vcs = config.modules.vcs();
        let mut envs = hashmap!{
            "DEPLO_CI_TYPE" => "GhAction".to_string(),
        };
        if config::Config::is_running_on_ci() {
            envs.insert(config::DEPLO_RUNNING_ON_CI_ENV_KEY, "true".to_string());
        }
        for key in vec!["GITHUB_OUTPUT", "GITHUB_ENV"] {
            match std::env::var(key) {
                Ok(v) => {
                    envs.insert(key, v);
                },
                Err(_) => if config::Config::is_running_on_ci(){
                    panic!("{} should set on CI env", key);
                }
            }
        }
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
    fn generate_token(&self, token_config: &ci::TokenConfig) -> Result<String, Box<dyn Error>> {
        match token_config {
            ci::TokenConfig::OIDC{audience: aud} => {
                let response = self.shell.exec(shell::args![
                    "curl", "-H",
                    shell::fmtargs!("Authorization: Bearer {}", value::Value::new_env("ACTIONS_ID_TOKEN_REQUEST_TOKEN")),
                    "-H", "Accept: application/vnd.github.v3+json",
                    shell::fmtargs!("{}&audience={}", value::Value::new_env("ACTIONS_ID_TOKEN_REQUEST_URL"), aud)
                ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
                let parsed = serde_json::from_str::<WebIdentityTokenResponse>(&response)?;
                Ok(parsed.value)
            }
        }
    }
    fn job_env(&self) -> HashMap<String, config::Value> {
        merge_hashmap(
            &match std::env::var("ACTIONS_ID_TOKEN_REQUEST_URL") {
                Ok(_) => hashmap!{
                    "ACTIONS_ID_TOKEN_REQUEST_URL".to_string() => 
                        config::value::Value::new_env("ACTIONS_ID_TOKEN_REQUEST_URL")
                },
                Err(_) => hashmap!{}
            }, &match std::env::var("ACTIONS_ID_TOKEN_REQUEST_TOKEN") {
                Ok(_) => hashmap!{
                    "ACTIONS_ID_TOKEN_REQUEST_TOKEN".to_string() => 
                        config::value::Value::new_env("ACTIONS_ID_TOKEN_REQUEST_TOKEN")
                },
                Err(_) => hashmap!{}
            }
        )
    }
    fn list_secret_name(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let config = self.config.borrow();
        let user_and_repo = config.modules.vcs().user_and_repo()?;
        let (token, auth_type) = self.get_token()?;
        let response = match serde_json::from_str::<RepositorySecretsResponse>(
            &self.shell.exec(shell::args![
            "curl", format!(
                "https://api.github.com/repos/{}/{}/actions/secrets",
                user_and_repo.0, user_and_repo.1
            ),
            "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token),
            "-H", "Accept: application/json"
        ], shell::no_env(), shell::no_cwd(), &shell::capture())?) {
            Ok(v) => v,
            Err(e) => return escalate!(Box::new(e))
        };
        Ok(
            response.secrets.iter().map(|s| s.name.clone()).collect()
        )
    }
    fn set_secret(&self, key: &str, value: &str, targets: &Option<Vec<String>>) -> Result<(), Box<dyn Error>> {
        let ts = match targets {
            Some(v) => v,
            None => &G_ALL_SECRET_TARGETS
        };
        for t in ts {
            self.set_secret_base(key, value, &t)?;
        }
        Ok(())
    }
    fn set_var(&self, key: &str, value: &str) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let user_and_repo = config.modules.vcs().user_and_repo()?;
        let (token, auth_type) = self.get_token()?;
        let json = format!("{{\"name\":\"{}\",\"value\":\"{}\"}}", 
            key, escape(value)
        );
        // Check if the variable already exists
        let check_status = self.shell.exec(shell::args!(
            "curl", "-X", "GET",
            format!(
                "https://api.github.com/repos/{}/{}/actions/variables/{}",
                user_and_repo.0, user_and_repo.1, key
            ),
            "-H", "Content-Type: application/json",
            "-H", "Accept: application/json",
            "-H", "X-GitHub-Api-Version: 2022-11-28",
            "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token),
            "-w", "%{http_code}", "-o", "/dev/null"
        ), shell::no_env(), shell::no_cwd(), &shell::capture())?.parse::<u32>()?;
        log::debug!("check status: {}", check_status);
        let (method, url) = if check_status != 200 { 
            ("POST", format!(
                "https://api.github.com/repos/{}/{}/actions/variables",
                user_and_repo.0, user_and_repo.1
            ))
        } else {
            ("PATCH", format!(
                "https://api.github.com/repos/{}/{}/actions/variables/{}",
                user_and_repo.0, user_and_repo.1, key
            ))
        };
        // TODO_PATH: use Path to generate path of /dev/null
        let status = self.shell.exec(shell::args!(
            "curl", "-X", method, url,
            "-H", "Content-Type: application/json",
            "-H", "Accept: application/json",
            "-H", "X-GitHub-Api-Version: 2022-11-28",
            "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token),
            "-d", json, "-w", "%{http_code}", "-o", "/dev/null"
        ), shell::no_env(), shell::no_cwd(), &shell::capture())?.parse::<u32>()?;
        if status >= 200 && status < 300 {
            Ok(())
        } else {
            return escalate!(Box::new(ci::CIError {
                cause: format!("fail to set variable to CircleCI CI with status code:{}", status)
            }));
        }
    }    
}