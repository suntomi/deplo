use std::fs;
use std::error::Error;
use std::result::Result;

use glob::glob;
use serde::{Deserialize, Serialize};

use crate::config;
use crate::ci;
use crate::shell;
use crate::module;
use crate::util::{escalate,seal,MultilineFormatString,rm};

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
    fn generate_container_setting<'a>(&self, container: &'a Option<String>) -> String {
        let container = container.map_or_else(|| "", |v| format!("container: {}", v);
        format!("{}/{}", self.account_name, container)
    }
    fn generate_checkout_steps<'a>(&self, job_name: &'a str, options: &'a Option<HashMap<String, String>>>) -> String {
        let checkout_opts = options.map_or_else(
            || "", 
            |v| v.iter().map(|(k,v)| {
                return if k == "lfs" {
                    format!("{}: {}", k, v))
                } else {
                    log::warn!("deplo only support lfs options for github action checkout but {}({}) is specified", k, v)
                    ""
                }
            }).collect::<Vec<String>>().join("\n")
        );
        format!(
            include_str!("../../res/ci/ghaction/checkout.yml.tmpl"), 
            name = job_name, checkout_opts = checkout_opts,
        )
    }
}

impl<'a, S: shell::Shell> module::Module for GhAction<S> {
    fn prepare(&self, reinit: bool) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let repository_root = config.vcs_service()?.repository_root()?;
        let mut jobs = config.select_jobs("GhAction")?;
        let create_main = config.is_main_ci("GhAction");
        if create_main {
            jobs["main"] = None
        }
        fs::create_dir_all(&format!("{}/.github/workflows", repository_root))?;
        if reinit {
            for (name, _) in &jobs {
                let yml_path = format!("{}/.github/workflows/deplo-{}.yml", repository_root, name);
                rm(&yml_path);
            }
        }
        // first time or reinit 
        let secrets_need_init = glob(format!("{}/.github/workflows/deplo-*.yml", repository_root)).next().is_none();
        // get target branch
        let target_branches = config.common.release_targets
            .values().map(|s| &**s)
            .collect::<Vec<&str>>().join(",");
        // inject secrets from dotenv file
        let mut secrets = vec!();
        config.parse_dotenv(|k,v| {
            if secrets_need_init {
                (self as &dyn ci::CI).set_secret(k, v)?;
            }
            Ok(secrets.push(format!("{}: ${{{{ secrets.{} }}}}", k, k)))
        })?;
        for (name, job) in &jobs {
            let yml_path = format!("{}/.github/workflows/deplo-{}.yml", repository_root, name);            
            match fs::metadata(&yml_path) {
                Ok(_) => log::debug!("config file for github workflow already created at {}". yml_path),
                Err(_) => {
                    fs::write(&deplo_yml_path, if name == "main" { 
                        format!(
                            include_str!("../../res/ci/ghaction/main.yml.tmpl"), target = target_branches, 
                            secrets = MultilineFormatString{ strings: &secrets, postfix: None },
                            image = config.common.deplo_image, tag = config::DEPLO_GIT_HASH
                            checkout = MultilineFormatString{
                                strings: self.generate_checkout_steps(name, None),
                                postfix: None
                            }
                        )
                    } else {
                        let j = job.unwrap(); //should exists always
                        format!(
                            include_str!("../../res/ci/ghaction/job.yml.tmpl"), name = name,
                            image = config.common.deplo_image, tag = config::DEPLO_GIT_HASH,
                            secrets = MultilineFormatString{ strings: &secrets, postfix: None },
                            machine = j.machine.unwarp_or_else("ubuntu-latest"),
                            container = self.generate_container_setting(j.container),
                            checkout = MultilineFormatString{
                                strings: self.generate_checkout_steps(name, j.checkout_opts),
                                postfix: None
                            },
                            sudo = j.container.is_some()
                        )
                    })?;
                }
            }
        }
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
        match std::env::var("DEPLO_GHACTION_PULL_REQUEST_URL") {
            Ok(v) => Ok(Some(v)),
            Err(e) => {
                match e {
                    std::env::VarError::NotPresent => Ok(None),
                    _ => return escalate!(Box::new(e))
                }
            }
        }
    }
    fn run_job(&self, job_name: &str) -> Result<String, Box<dyn Error>> {
        Ok("".to_string())
    }
    fn wait_job(&self, job_id: &str) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn wait_job_by_name(&self, job_name: &str) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn set_secret(&self, key: &str, _: &str) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let token = match &config.ci_config(&self.account_name) {
            config::CIConfig::GhAction { account:_, key, workflow:_ } => { key },
            config::CIConfig::CircleCI{..} => { 
                return escalate!(Box::new(ci::CIError {
                    cause: "should have ghaction CI config but circleci config provided".to_string()
                }));
            }
        };
        let user_and_repo = config.vcs_service()?.user_and_repo()?;
        let public_key_info = serde_json::from_str::<RepositoryPublicKeyResponse>(
            &self.shell.eval_output_of(&format!(r#"
                curl https://api.github.com/repos/{}/{}/actions/secrets/public-key?access_token={}
            "#, user_and_repo.0, user_and_repo.1, token), shell::no_env())?
        )?;
        let json = format!("{{\"encrypted_value\":\"{}\",\"key_id\":\"{}\"}}", 
            //get value from env to unescapse
            seal(&std::env::var(key).unwrap(), &public_key_info.key)?,
            public_key_info.key_id
        );
        let status = self.shell.exec(&vec!(
            "curl", "-X", "PUT",
            &format!(
                "https://api.github.com/repos/{}/{}/actions/secrets/{}?access_token={}",
                user_and_repo.0, user_and_repo.1, key, token
            ),
            "-H", "Content-Type: application/json",
            "-H", "Accept: application/json",
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