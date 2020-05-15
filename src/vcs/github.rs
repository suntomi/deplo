use std::error::Error;
use std::result::Result;

use regex::Regex;

use crate::config;
use crate::vcs;
use super::git;

pub struct Github<'a> {
    pub config: &'a config::Config<'a>,
    pub git: git::Git<'a>,
}

impl<'a> vcs::VCS<'a> for Github<'a> {
    fn new(config: &'a config::Config) -> Result<Github<'a>, Box<dyn Error>> {
        return Ok(Github::<'a> {
            config: config,
            git: git::Git::<'a>::new(config)
        });
    }
    fn release_target(&self) -> Option<String> {
        let b = self.git.current_branch().unwrap();
        for (k,v) in &self.config.common.release_targets {
            let re = regex::Regex::new(v).unwrap();
            match re.captures(&b) {
                Some(_) => return Some(k.to_string()),
                None => {}, 
            }
        }
        None
    }
}
