use std::error::Error;
use std::collections::HashMap;
use std::result::Result;

use regex;

use crate::config;
use crate::vcs;
use crate::module;
use super::git;

pub struct Github<GIT: (git::GitFeatures) + (git::GitHubFeatures) = git::Git> {
    pub config: config::Container,
    pub git: GIT,
    pub diff: Vec<String>
}

impl<GIT: (git::GitFeatures) + (git::GitHubFeatures)> Github<GIT> {
    fn push_url(&self) -> Result<String, Box<dyn Error>> {
        let config = self.config.borrow();
        if let config::VCSConfig::Github{ email:_, account, key } = &config.vcs {
            let user_and_repo = (self as &dyn vcs::VCS).user_and_repo()?;
            Ok(format!("https://{}:{}@github.com/{}/{}", account, key, user_and_repo.0, user_and_repo.1))
        } else {
            Err(Box::new(vcs::VCSError {
                cause: format!("should have github config, got: {}", config.vcs)
            }))
        }
    }
}

impl<GIT: (git::GitFeatures) + (git::GitHubFeatures)> module::Module for Github<GIT> {
    fn prepare(&self, _:bool) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}

impl<GIT: (git::GitFeatures) + (git::GitHubFeatures)> vcs::VCS for Github<GIT> {
    fn new(config: &config::Container) -> Result<Github<GIT>, Box<dyn Error>> {
        if let config::VCSConfig::Github{ account, key:_, email } = &config.borrow().vcs {
            let mut gh = Github {
                config: config.clone(),
                diff: vec!(),
                git: GIT::new(&account, &email, config)
            };
            let diff = gh.git.rebase_with_remote_counterpart(&gh.push_url()?, &gh.git.current_branch()?)?;
            gh.diff = diff.split('\n').map(|e| e.to_string()).collect();
            return Ok(gh)
        } 
        return Err(Box::new(vcs::VCSError {
            cause: format!("should have github config but {}", config.borrow().vcs)
        }))
    }
    fn release_target(&self) -> Option<String> {
        let config = self.config.borrow();
        let b = self.git.current_branch().unwrap();
        for (k,v) in &config.common.release_targets {
            let re = regex::Regex::new(v).unwrap();
            match re.captures(&b) {
                Some(_) => return Some(k.to_string()),
                None => {}, 
            }
        }
        None
    }
    fn rebase_with_remote_counterpart(&self, branch: &str) -> Result<String, Box<dyn Error>> {
        self.git.rebase_with_remote_counterpart(&self.push_url()?, branch)
    }
    fn current_branch(&self) -> Result<String, Box<dyn Error>> {
        self.git.current_branch()
    }
    fn commit_hash(&self) -> Result<String, Box<dyn Error>> {
        self.git.commit_hash()
    }
    fn repository_root(&self) -> Result<String, Box<dyn Error>> {
        self.git.repository_root()
    }
    fn push(
        &self, branch: &str, msg: &str, patterns: &Vec<&str>, options: &HashMap<&str, &str>
    ) -> Result<bool, Box<dyn Error>> {
        self.git.push(&self.push_url()?, branch, msg, patterns, options)
    }
    fn pr(
        &self, title: &str, head_branch: &str, base_branch: &str, options: &HashMap<&str, &str>
    ) -> Result<(), Box<dyn Error>> {
        self.git.pr(title, head_branch, base_branch, options)
    }
    fn user_and_repo(&self) -> Result<(String, String), Box<dyn Error>> {
        let remote_origin = self.git.remote_origin()?;
        let re = regex::Regex::new(r"[^:]+[:/]([^/\.]+)/([^/\.]+)").unwrap();
        let user_and_repo = match re.captures(&remote_origin) {
            Some(c) => (
                c.get(1).map_or("".to_string(), |m| m.as_str().to_string()), 
                c.get(2).map_or("".to_string(), |m| m.as_str().to_string())
            ),
            None => return Err(Box::new(vcs::VCSError {
                cause: format!("invalid remote origin url: {}", remote_origin)
            }))
        };
        Ok(user_and_repo)
    }
    fn diff<'b>(&'b self) -> &'b Vec<String> {
        &self.diff
    }
}
