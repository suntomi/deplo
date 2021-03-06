use std::fs;
use std::path;
use std::error::Error;
use std::result::Result;

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

impl<'a, S: shell::Shell> module::Module for GhAction<S> {
    fn prepare(&self, reinit: bool) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let repository_root = config.vcs_service()?.repository_root()?;
        let deplo_yml_path = format!("{}/.github/workflows/deplo.yml", repository_root);
        let deplo_osx_yml_path = format!("{}/.github/workflows/deplo-osx.yml", repository_root);
        if reinit {
            rm(&deplo_yml_path);
            rm(&deplo_osx_yml_path);
        }
        match fs::metadata(&deplo_yml_path) {
            Ok(_) => log::debug!("config file for github action already created"),
            Err(_) => {
                let target_branches = config.common.release_targets
                    .values().map(|s| &**s)
                    .collect::<Vec<&str>>().join(",");
                let cli_opts = config.ci_cli_options();
                let mut env_inject_settings = vec!();
                config.parse_dotenv(|k,v| {
                    (self as &dyn ci::CI).set_secret(k, v)?;
                    Ok(env_inject_settings.push(format!("{}: ${{{{ secrets.{} }}}}", k, k)))
                })?;
                fs::create_dir_all(&format!("{}/.github/workflows", repository_root))?;
                fs::write(&deplo_yml_path, format!(
                    include_str!("../../rsc/ci/ghaction/deplo.yml.tmpl"), 
                    target_branches, target_branches, 
                    MultilineFormatString{ strings: &env_inject_settings, postfix: None },
                    config.common.deplo_image, config::DEPLO_GIT_HASH,
                    cli_opts, cli_opts
                ))?;
                fs::write(&deplo_osx_yml_path, format!(
                    include_str!("../../rsc/ci/ghaction/deplo-osx.yml.tmpl"), 
                    MultilineFormatString{ strings: &env_inject_settings, postfix: None },
                    config.common.deplo_image, config::DEPLO_GIT_HASH,
                    cli_opts
                ))?;
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
            config::CIConfig::GhAction { account:_, key, action:_ } => { key },
            config::CIConfig::Circle{ key:_, action:_ } => { 
                return escalate!(Box::new(ci::CIError {
                    cause: "should have ghaction CI config but circle config provided".to_string()
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
                cause: format!("fail to set secret to Circle CI with status code:{}", status)
            }));
        }
    }
}
