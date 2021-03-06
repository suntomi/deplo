use std::fs;
use std::path;
use std::error::Error;
use std::result::Result;

use crate::config;
use crate::ci;
use crate::shell;
use crate::module;
use crate::util::{escalate,rm};

pub struct Circle<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub account_name: String,
    pub shell: S,
}

impl<'a, S: shell::Shell> module::Module for Circle<S> {
    fn prepare(&self, reinit: bool) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let repository_root = config.vcs_service()?.repository_root()?;
        let circle_yml_path = format!("{}/.circleci/config.yml", repository_root);
        let cli_opts = config.ci_cli_options();
        if reinit {
            rm(&circle_yml_path);
        }
        match fs::metadata(&circle_yml_path) {
            Ok(_) => log::debug!("config file for circle ci already created"),
            Err(_) => {
                // sync dotenv secrets with ci system
                config.parse_dotenv(|k,v| (self as &dyn ci::CI).set_secret(k, v))?;
                fs::create_dir_all(&format!("{}/.circleci", repository_root))?;
                fs::write(&circle_yml_path, format!(
                    include_str!("../../rsc/ci/circle/config.yml.tmpl"),
                    config.common.deplo_image,
                    config::DEPLO_GIT_HASH, 
                    cli_opts, cli_opts
                ))?;
            }
        }
        Ok(())
    }
}

impl<'a, S: shell::Shell> ci::CI for Circle<S> {
    fn new(config: &config::Container, account_name: &str) -> Result<Circle<S>, Box<dyn Error>> {
        return Ok(Circle::<S> {
            config: config.clone(),
            account_name: account_name.to_string(),
            shell: S::new(config),
        });
    }
    fn pull_request_url(&self) -> Result<Option<String>, Box<dyn Error>> {
        match std::env::var("CIRCLE_PULL_REQUEST") {
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
    fn set_secret(&self, key: &str, val: &str) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let token = match &config.ci_config(&self.account_name) {
            config::CIConfig::Circle { key, action:_ } => { key },
            config::CIConfig::GhAction{ account:_, key:_, action:_ } => { 
                return escalate!(Box::new(ci::CIError {
                    cause: "should have circle CI config but ghaction config provided".to_string()
                }));
            }
        };
        let json = format!("{{\"name\":\"{}\",\"value\":\"{}\"}}", key, val);
        let user_and_repo = config.vcs_service()?.user_and_repo()?;
        let status = self.shell.exec(&vec!(
            "curl", "-X", "POST", "-u", &format!("{}:", token),
            &format!(
                "https://circleci.com/api/v2/project/gh/{}/{}/envvar",
                user_and_repo.0, user_and_repo.1
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
