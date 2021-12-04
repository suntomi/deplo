use std::fs;
use std::error::Error;
use std::result::Result;
use std::collections::{HashMap};

use log;
use serde::{Deserialize, Serialize};

use crate::config;
use crate::ci;
use crate::shell;
use crate::module;
use crate::util::{escalate,seal,MultilineFormatString,rm,maphash,sorted_key_iter};

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
            .map(|(_,v)| &**v)
            .collect::<Vec<&str>>().join(",");
        format!(
            include_str!("../../res/ci/ghaction/entrypoint.yml.tmpl"), 
            targets = target_branches
        ).split("\n").map(|s| s.to_string()).collect()
    }
    fn generate_outputs<'a>(&self, jobs: &HashMap<(&'a str, &'a str), &'a config::Job>) -> Vec<String> {
        sorted_key_iter(jobs).map(|(v,_)| {
            format!("{kind}-{name}: steps.deplo-ci-kick.{kind}-{name}", kind = v.0, name = v.1)
        }).collect()
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
            config::Runner::Machine{ image:_, os:_, class:_ } => vec![],
            config::Runner::Container{ image } => vec![format!("container: {}", image)]
        }
    }
    fn generate_checkout_steps<'a>(&self, _: &'a str, options: &'a Option<HashMap<String, String>>) -> Vec<String> {
        let checkout_opts = options.as_ref().map_or_else(
            || vec![], 
            |v| v.iter().map(|(k,v)| {
                return if k == "lfs" {
                    format!("{}: {}", k, v)
                } else {
                    format!("# warning: deplo only support lfs options for github action checkout but {}({}) is specified", k, v)
                }
            }).collect::<Vec<String>>()
        );
        // hash value for separating repository cache according to checkout options
        let opts_hash = options.as_ref().map_or_else(
            || "".to_string(), 
            |v| { format!("-{}", maphash(v)) }
        );
        format!(
            include_str!("../../res/ci/ghaction/checkout.yml.tmpl"), 
            checkout_opts = MultilineFormatString{
                strings: &checkout_opts,
                postfix: None
            }, opts_hash = opts_hash
        ).split("\n").map(|s| s.to_string()).collect()
    }
}

impl<'a, S: shell::Shell> module::Module for GhAction<S> {
    fn prepare(&self, reinit: bool) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let repository_root = config.vcs_service()?.repository_root()?;
        let workflow_yml_path = format!("{}/.github/workflows/deplo-main.yml", repository_root);
        let create_main = config.is_main_ci("GhAction");
        fs::create_dir_all(&format!("{}/.github/workflows", repository_root))?;
        let previously_no_file = !rm(&workflow_yml_path);
        // inject secrets from dotenv file
        let mut secrets = vec!();
        config.parse_dotenv(|k,v| {
            if previously_no_file || reinit {
                (self as &dyn ci::CI).set_secret(k, v)?;
                log::info!("set secret value of {}", k);
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
                include_str!("../../res/ci/ghaction/job.yml.tmpl"), name = name,
                needs = self.generate_job_dependencies(names.0, &job.depends),
                machine = match job.runner {
                    config::Runner::Machine{ref image, ref os, class:_} => match image {
                        Some(v) => v.as_str(),
                        None => match os {
                            config::RunnerOS::Linux => "ubuntu-latest",
                            config::RunnerOS::Windows => "macos-latest",
                            config::RunnerOS::MacOS => "windows-latest",
                        }
                    },
                    config::Runner::Container{image:_} => "ubuntu-latest",
                },
                container = MultilineFormatString{
                    strings: &self.generate_container_setting(&job.runner),
                    postfix: None
                },
                checkout = MultilineFormatString{
                    strings: &self.generate_checkout_steps(&name, &job.checkout),
                    postfix: None
                },
                //if in container, sudo does not required to install debug instrument
                sudo = job.runs_on_machine()
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
                image = config.deplo_image(), tag = config::DEPLO_GIT_HASH,
                checkout = MultilineFormatString{
                    strings: &self.generate_checkout_steps("main", &None),
                    postfix: None
                },
                jobs = MultilineFormatString{
                    strings: &job_descs,
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
    fn pull_request_url(&self) -> Result<Option<String>, Box<dyn Error>> {
        match std::env::var("DEPLO_CI_PULL_REQUEST_URL") {
            Ok(v) => Ok(Some(v)),
            Err(e) => {
                match e {
                    std::env::VarError::NotPresent => Ok(None),
                    _ => return escalate!(Box::new(e))
                }
            }
        }
    }
    fn run_job(&self, _: &str) -> Result<String, Box<dyn Error>> {
        log::warn!("TODO: implement run_job for ghaction");
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
            &self.shell.eval_output_of(&format!(r#"
                curl https://api.github.com/repos/{}/{}/actions/secrets/public-key \
                -H "Authorization: token {}"
            "#, user_and_repo.0, user_and_repo.1, token), shell::no_env()
        )?) {
            Ok(v) => v,
            Err(e) => return escalate!(Box::new(e))
        };
        let json = format!("{{\"encrypted_value\":\"{}\",\"key_id\":\"{}\"}}", 
            //get value from env to unescapse
            seal(&std::env::var(key).unwrap(), &public_key_info.key)?,
            public_key_info.key_id
        );
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
        ), shell::no_env(), true)?.parse::<u32>()?;
        if status >= 200 && status < 300 {
            Ok(())
        } else {
            return escalate!(Box::new(ci::CIError {
                cause: format!("fail to set secret to CircleCI CI with status code:{}", status)
            }));
        }
    }
}