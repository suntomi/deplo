use std::fs;
use std::error::Error;
use std::result::Result;
use std::collections::{HashMap};

use log;
use maplit::hashmap;
use serde::{Deserialize, Serialize};

use crate::config;
use crate::ci;
use crate::shell;
use crate::module;
use crate::util::{
    escalate,seal,
    MultilineFormatString,rm,
    maphash,
    sorted_key_iter,
    merge_hashmap
};

#[derive(Serialize, Deserialize)]
struct RepositoryPublicKeyResponse {
    key: String,
    key_id: String,
}

pub struct GhAction<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub account_name: String,
    pub shell: S,
}

impl<S: shell::Shell> GhAction<S> {
    fn generate_entrypoint<'a>(&self, config: &'a config::Config) -> Vec<String> {
        // get target branch
        let target_branches = sorted_key_iter(&config.common.release_targets)
            .filter(|v| v.1.is_branch())
            .map(|(_,v)| v.path())
            .collect::<Vec<&str>>();
        let target_tags = sorted_key_iter(&config.common.release_targets)
            .filter(|v| v.1.is_tag())
            .map(|(_,v)| v.path())
            .collect::<Vec<&str>>();
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
    fn generate_outputs<'a>(&self, jobs: &HashMap<(&'a str, &'a str), &'a config::Job>) -> Vec<String> {
        sorted_key_iter(jobs).map(|(v,_)| {
            format!("{kind}-{name}: ${{{{ steps.deplo-ci-kick.outputs.{kind}-{name} }}}}", kind = v.0, name = v.1)
        }).collect()
    }
    fn generate_debugger(&self, job: Option<&config::Job>, config: &config::Config) -> Vec<String> {
        let sudo = match job {
            Some(ref j) => {
                if !config.common.debug.as_ref().map_or_else(|| false, |v| v.get("ghaction_job_debugger").is_some()) &&
                    !j.options.as_ref().map_or_else(|| false, |v| v.get("debugger").is_some()) {
                    return vec![];
                }
                // if in container, sudo does not required to install debug instrument
                match j.runner {
                    config::Runner::Machine{..} => true,
                    config::Runner::Container{..} => false,        
                }
            },
            None => {
                // deplo kick/finish
                if !config.common.debug.as_ref().map_or_else(|| false, |v| v.get("ghaction_deplo_debugger").is_some()) {
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
    fn generate_caches(&self, job: &config::Job) -> Vec<String> {
        match job.caches {
            Some(ref c) => sorted_key_iter(c).map(|(name,cache)| {
                format!(
                    include_str!("../../res/ci/ghaction/cache.yml.tmpl"), 
                    name = name, key = cache.keys[0], 
                    restore_keys = MultilineFormatString{
                        strings: &cache.keys[1..].to_vec(), postfix: None
                    },
                    paths = MultilineFormatString{
                        strings: &cache.paths, postfix: None
                    },
                    env_key = format!("DEPLO_CACHE_{}_HIT", name.to_uppercase())
                ).split("\n").map(|s| s.to_string()).collect::<Vec<String>>()
            }).collect(),
            None => vec![]
        }.concat()
    }
    fn generate_command<'a>(&self, names: &(&str, &str), job: &'a config::Job) -> Vec<String> {
        match job.runner {
            config::Runner::Machine{ref os, ..} => match os {
                config::RunnerOS::Linux => return vec![format!("run: deplo ci {} {}", names.0, names.1)],
                config::RunnerOS::Windows => (),
                config::RunnerOS::MacOS => (),
            },
            config::Runner::Container{image:_} => return vec![format!("run: deplo ci {} {}", names.0, names.1)],
        };
        format!(include_str!("../../res/ci/ghaction/rawexec.yml.tmpl"),
            scripts = MultilineFormatString{
                strings: &job.command.split("\n").map(|s| s.to_string()).collect(),
                postfix: None
            }
        ).split("\n").map(|s| s.to_string()).collect()
    }
    fn generate_job_dependencies<'a>(&self, kind: &'a str, depends: &'a Option<Vec<String>>) -> String {
        depends.as_ref().map_or_else(
            || "deplo-main".to_string(),
            |v| {
                let mut vs = v.iter().map(|d| {
                    format!("{}-{}", kind, d)
                }).collect::<Vec<String>>();
                vs.push("deplo-main".to_string());
                format!("\"{}\"", vs.join("\",\""))
            })
    }
    fn generate_container_setting<'a>(&self, runner: &'a config::Runner) -> Vec<String> {
        match runner {
            config::Runner::Machine{ .. } => vec![],
            config::Runner::Container{ image } => vec![format!("container: {}", image)]
        }
    }
    fn generate_fetchcli_steps<'a>(&self, runner: &'a config::Runner) ->Vec<String> {
        let uname = match runner {
            config::Runner::Machine{ref os, ..} => match os {
                config::RunnerOS::Windows => return vec![],
                v => v.uname()
            },
            config::Runner::Container{image:_} => "Linux",
        };
        format!(include_str!("../../res/ci/ghaction/fetchcli.yml.tmpl"),
            deplo_cli_path = "/usr/local/bin/deplo",
            download_url = format!(
                "{}/{}/deplo-{}",
                config::DEPLO_RELEASE_URL_BASE, config::DEPLO_VERSION, uname
            )
        ).split("\n").map(|s| s.to_string()).collect::<Vec<String>>()
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
        &self, _: &'a str, options: &'a Option<HashMap<String, String>>, defaults: &Option<HashMap<String, String>>
    ) -> Vec<String> {
        let merged_opts = options.as_ref().map_or_else(
            || defaults.clone().unwrap_or(HashMap::new()),
            |v| merge_hashmap(&defaults.clone().unwrap_or(HashMap::new()), v)
        );
        let checkout_opts = merged_opts.iter().map(|(k,v)| {
            return if vec!["fetch-depth", "lfs"].contains(&k.as_str()) {
                format!("{}: {}", k, v)
            } else {
                format!("# warning: deplo only support lfs options for github action checkout but {}({}) is specified", k, v)
            }
        }).collect::<Vec<String>>();
        // hash value for separating repository cache according to checkout options
        let opts_hash = options.as_ref().map_or_else(
            || "".to_string(), 
            |v| { format!("-{}", maphash(v)) }
        );
        if merged_opts.get("fetch-depth").map_or_else(|| false, |v| v.parse::<i32>().unwrap_or(-1) == 0) {
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
}

impl<'a, S: shell::Shell> module::Module for GhAction<S> {
    fn prepare(&self, reinit: bool) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let repository_root = config.vcs_service()?.repository_root()?;
        // TODO_PATH: use Path to generate path of /.github/...
        let workflow_yml_path = format!("{}/.github/workflows/deplo-main.yml", repository_root);
        let create_main = config.is_main_ci("GhAction");
        fs::create_dir_all(&format!("{}/.github/workflows", repository_root))?;
        let previously_no_file = !rm(&workflow_yml_path);
        // inject secrets from dotenv file
        let mut secrets = vec!();
        config.parse_dotenv(|k,v| {
            if previously_no_file || reinit {
                (self as &dyn ci::CI).set_secret(k, v)?;
                log::debug!("set secret value of {}", k);
            }
            Ok(secrets.push(format!("{}: ${{{{ secrets.{} }}}}", k, k)))
        })?;
        // generate job entries
        let mut job_descs = Vec::new();
        let mut all_job_names = vec!["deplo-main".to_string()];
        let jobs = config.enumerate_jobs();
        for (names, job) in sorted_key_iter(&jobs) {
            let name = format!("{}-{}", names.0, names.1);
            all_job_names.push(name.clone());
            let lines = format!(
                include_str!("../../res/ci/ghaction/job.yml.tmpl"), 
                full_name = name, kind = names.0, name = names.1,
                needs = self.generate_job_dependencies(names.0, &job.depends),
                machine = match job.runner {
                    config::Runner::Machine{ref image, ref os, ..} => match image {
                        Some(v) => v.as_str(),
                        None => match os {
                            config::RunnerOS::Linux => "ubuntu-latest",
                            config::RunnerOS::Windows => "windows-latest",
                            config::RunnerOS::MacOS => "macos-latest",
                        }
                    },
                    config::Runner::Container{image:_} => "ubuntu-latest",
                },
                caches = MultilineFormatString{
                    strings: &self.generate_caches(&job),
                    postfix: None
                },
                command = MultilineFormatString{
                    strings: &self.generate_command(&names, &job),
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
                    strings: &self.generate_checkout_steps(&name, &job.checkout, &None),
                    postfix: None
                },
                debugger = MultilineFormatString{
                    strings: &self.generate_debugger(Some(&job), &config),
                    postfix: None
                }
            ).split("\n").map(|s| s.to_string()).collect::<Vec<String>>();
            job_descs = job_descs.into_iter().chain(lines.into_iter()).collect();
        }
        fs::write(&workflow_yml_path,
            format!(
                include_str!("../../res/ci/ghaction/main.yml.tmpl"), 
                entrypoint = MultilineFormatString{ 
                    strings: &(if create_main { self.generate_entrypoint(&config) } else { vec![] }),
                    postfix: None
                },
                secrets = MultilineFormatString{ strings: &secrets, postfix: None },
                outputs = MultilineFormatString{ 
                    strings: &self.generate_outputs(&jobs),
                    postfix: None
                },
                fetchcli = MultilineFormatString{
                    strings: &self.generate_fetchcli_steps(&config::Runner::Machine{
                        os: config::RunnerOS::Linux, image: None, class: None, local_fallback: None }
                    ),
                    postfix: None
                },
                checkout = MultilineFormatString{
                    strings: &self.generate_checkout_steps("main", &None, &Some(hashmap!{
                        "fetch-depth".to_string() => "2".to_string()
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
                needs = format!("\"{}\"", all_job_names.join("\",\""))
            )
        )?;
        Ok(())
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
    fn kick(&self) -> Result<(), Box<dyn Error>> {
        println!("::set-output name=DEPLO_OUTPUT_CLI_VERSION::{}", config::DEPLO_VERSION);
        Ok(())
    }
    fn pull_request_url(&self) -> Result<Option<String>, Box<dyn Error>> {
        match std::env::var("DEPLO_CI_PULL_REQUEST_URL") {
            Ok(v) => if v.is_empty() { Ok(None) } else { Ok(Some(v)) },
            Err(e) => {
                match e {
                    std::env::VarError::NotPresent => Ok(None),
                    _ => return escalate!(Box::new(e))
                }
            }
        }
    }
    fn mark_job_executed(&self, job_name: &str) -> Result<(), Box<dyn Error>> {
        if config::Config::is_running_on_ci() {
            println!("::set-output name={}::true", job_name);
        } else {
            self.config.borrow().run_job_by_name(&self.shell, job_name)?;
        }
        Ok(())
    }
    fn run_job(&self, _: &str) -> Result<String, Box<dyn Error>> {
        Ok("".to_string())
    }
    fn wait_job(&self, _: &str) -> Result<(), Box<dyn Error>> {
        log::warn!("TODO: implement wait_job for ghaction");
        Ok(())
    }
    fn wait_job_by_name(&self, _: &str) -> Result<(), Box<dyn Error>> {
        log::warn!("TODO: implement wait_job_by_name for ghaction");
        Ok(())
    }
    fn job_env(&self) -> HashMap<&str, String> {
        let config = self.config.borrow();
        let mut envs = hashmap!{
            // DEPLO_CI_PULL_REQUEST_URL is set by generated deplo-main.yml by default
            //TODO_CI: need to get pr URL value on local execution
            "DEPLO_CI_PULL_REQUEST_URL" => std::env::var("DEPLO_CI_PULL_REQUEST_URL").unwrap_or_else(|_| "".to_string()),
            "DEPLO_CI_TYPE" => "GhAction".to_string(),
            "DEPLO_CI_CURRENT_SHA" => std::env::var("GITHUB_SHA").unwrap_or_else(
                |_| config.vcs_service().unwrap().commit_hash(None).unwrap()
            ),
        };
        match std::env::var("GITHUB_REF_TYPE") {
            Ok(ref_type) => {
                match std::env::var("GITHUB_REF") {
                    Ok(ref_name) => {
                        match ref_type.as_str() {
                            "branch" => envs.insert("DEPLO_CI_BRANCH_NAME", ref_name),
                            "tag" => envs.insert("DEPLO_CI_TAG_NAME", ref_name),
                            v => panic!("invalid ref_type {}", v),
                        };
                    },
                    Err(_) => panic!("GITHUB_REF_TYPE is set but GITHUB_REF is not set"),
                }
            },
            Err(_) => {
                let (name, is_branch) = config.vcs_service().unwrap().current_branch().unwrap();
                if is_branch {
                    envs.insert("DEPLO_CI_BRANCH_NAME", name);
                } else {
                    envs.insert("DEPLO_CI_TAG_NAME", name);
                };
            }
        };
        envs
    }
    fn set_secret(&self, key: &str, _: &str) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let token = match &config.ci_config(&self.account_name) {
            config::CIAccount::GhAction { account:_, key } => { key },
            config::CIAccount::CircleCI{..} => { 
                return escalate!(Box::new(ci::CIError {
                    cause: "should have ghaction CI config but circleci config provided".to_string()
                }));
            }
        };
        let user_and_repo = config.vcs_service()?.user_and_repo()?;
        let public_key_info = match serde_json::from_str::<RepositoryPublicKeyResponse>(
            &self.shell.exec(&vec![
                "curl", &format!("https://api.github.com/repos/{}/{}/actions/secrets/public-key", user_and_repo.0, user_and_repo.1),
                "-H", &format!("Authorization: token {}", token)
            ], shell::no_env(), shell::no_cwd(), true
        )?) {
            Ok(v) => v,
            Err(e) => return escalate!(Box::new(e))
        };
        let json = format!("{{\"encrypted_value\":\"{}\",\"key_id\":\"{}\"}}", 
            //get value from env to unescapse
            seal(&std::env::var(key).unwrap(), &public_key_info.key)?,
            public_key_info.key_id
        );
        // TODO_PATH: use Path to generate path of /dev/null
        let status = self.shell.exec(&vec!(
            "curl", "-X", "PUT",
            &format!(
                "https://api.github.com/repos/{}/{}/actions/secrets/{}",
                user_and_repo.0, user_and_repo.1, key
            ),
            "-H", "Content-Type: application/json",
            "-H", "Accept: application/json",
            "-H", &format!("Authorization: token {}", token),
            "-d", &json, "-w", "%{http_code}", "-o", "/dev/null"
        ), shell::no_env(), shell::no_cwd(), true)?.parse::<u32>()?;
        if status >= 200 && status < 300 {
            Ok(())
        } else {
            return escalate!(Box::new(ci::CIError {
                cause: format!("fail to set secret to CircleCI CI with status code:{}", status)
            }));
        }
    }
}