use std::fs;
use std::error::Error;
use std::result::Result;

use maplit::hashmap;

use crate::config;
use crate::ci;
use crate::shell;
use crate::util::escalate;

pub struct Circle<'a, S: shell::Shell<'a> = shell::Default<'a>> {
    pub config: &'a config::Config<'a>,
    pub diff: String,
    pub shell: S,
}

impl<'a, S: shell::Shell<'a>> ci::CI<'a> for Circle<'a, S> {
    fn new(config: &'a config::Config) -> Result<Circle<'a, S>, Box<dyn Error>> {
        let vcs = config.vcs_service()?;
        return Ok(Circle::<'a, S> {
            config: config,
            shell: S::new(config),
            diff: vcs.rebase_with_remote_counterpart(&vcs.current_branch()?)?
        });
    }
    fn init(&self) -> Result<(), Box<dyn Error>> {
        let repository_root = self.config.vcs_service()?.repository_root()?;
        let circle_yml_path = format!("{}/.circleci/config.yml", repository_root);
        fs::create_dir_all(&format!("{}/.circleci", repository_root))?;
        fs::write(&circle_yml_path, format!(
            include_str!("../../rsc/ci/circle/config.yml.tmpl"),
            config::DEPLO_GIT_HASH, self.config.runtime.workdir.as_ref().unwrap_or(&"".to_string())
        ))?;
        Ok(())
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
    fn changed(&self, patterns: &Vec<&str>) -> bool {
        true
    }
    fn set_secret(&self, key: &str, val: &str) -> Result<(), Box<dyn Error>> {
        let token = match &self.config.ci {
            config::CIConfig::Circle { key } => { key },
            config::CIConfig::GhAction{ account:_, key:_ } => { 
                return escalate!(Box::new(ci::CIError {
                    cause: "should have circle CI config but ghaction config provided".to_string()
                }));
            }
        };
        let json = format!("{{\"name\":\"{}\",\"value\":\"{}\"}}", key, val);
        let user_and_repo = self.config.vcs_service()?.user_and_repo()?;
        let status = self.shell.exec(&vec!(
            "curl", "-X", "POST", "-u", &format!("{}:", token),
            &format!(
                "https://circleci.com/api/v2/project/gh/{}/{}/envvar",
                user_and_repo.0, user_and_repo.1
            ),
            "-H", "Content-Type: application/json",
            "-H", "Accept: application/json",
            "-d", &json, "-w", "%{http_code}", "-o", "/dev/null"
        ), &hashmap!{}, true)?.parse::<u32>()?;
        if status >= 200 && status < 300 {
            Ok(())
        } else {
            return escalate!(Box::new(ci::CIError {
                cause: format!("fail to set secret to Circle CI with status code:{}", status)
            }));
        }
    }
}
