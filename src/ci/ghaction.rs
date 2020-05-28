use std::error::Error;
use std::result::Result;

use crate::config;
use crate::ci;

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
