use std::fs;
use std::error::Error;
use std::result::Result;

use crate::config;
use crate::ci;

pub struct Circle<'a> {
    pub config: &'a config::Config<'a>,
    pub diff: String,
}

impl<'a> ci::CI<'a> for Circle<'a> {
    fn new(config: &'a config::Config) -> Result<Circle<'a>, Box<dyn Error>> {
        let vcs = config.vcs_service()?;
        return Ok(Circle::<'a> {
            config: config,
            diff: vcs.rebase_with_remote_counterpart(&vcs.current_branch()?)?
        });
    }
    fn init(&self) -> Result<(), Box<dyn Error>> {
        let repository_root = self.config.vcs_service()?.repository_root()?;
        let circle_yml_path = format!("{}/.circleci/config.yml", repository_root);
        fs::create_dir_all(&format!("{}/.circleci", repository_root))?;
        fs::write(&circle_yml_path, format!(
            include_str!("../../rsc/ci/circle/config.yml.tmpl"), config::DEPLO_GIT_HASH
        ))?;
        Ok(())
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
