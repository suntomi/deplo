use std::fs;
use std::error::Error;
use std::result::Result;

use crate::config;
use crate::ci;
use crate::util::escalate;

pub struct GhAction<'a> {
    pub config: &'a config::Config<'a>,
    pub diff: String
}

impl<'a> ci::CI<'a> for GhAction<'a> {
    fn new(config: &'a config::Config) -> Result<GhAction<'a>, Box<dyn Error>> {
        let vcs = config.vcs_service()?;
        return Ok(GhAction::<'a> {
            config: config,
            diff: vcs.rebase_with_remote_counterpart(&vcs.current_branch()?)?
        });
    }
    fn init(&self) -> Result<(), Box<dyn Error>> {
        let repository_root = self.config.vcs_service()?.repository_root()?;
        let deplo_yml_path = format!("{}/.github/workflows/deplo.yml", repository_root);
        let target_branches = self.config.common.release_targets
            .values().map(|s| &**s)
            .collect::<Vec<&str>>().join(",");
        fs::create_dir_all(&format!("{}/.github/workflows", repository_root))?;
        fs::write(&deplo_yml_path, format!(
            include_str!("../../rsc/ci/ghaction/deplo.yml.tmpl"), 
            target_branches, target_branches, config::DEPLO_GIT_HASH,
            self.config.runtime.workdir.as_ref().unwrap_or(&"".to_string())
        ))?;
        Ok(())
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
    fn changed(&self, patterns: &Vec<&str>) -> bool {
        true
    }
}
