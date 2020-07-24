use std::error::Error;
use std::collections::HashMap;
use std::result::Result;

use regex;

use crate::config;
use crate::vcs;
use super::git;

pub struct Github<'a, GIT: (git::GitFeatures<'a>) + (git::GitHubFeatures<'a>) = git::Git<'a>> {
    pub config: &'a config::Config,
    pub git: GIT,
    pub diff: Vec<String>
}

impl<'a, GIT: (git::GitFeatures<'a>) + (git::GitHubFeatures<'a>)> Github<'a, GIT> {
    fn push_url(&self) -> Result<String, Box<dyn Error>> {
        if let config::VCSConfig::Github{ email:_, account, key } = &self.config.vcs {
            let user_and_repo = (self as &dyn vcs::VCS<'a>).user_and_repo()?;
            Ok(format!("https://{}:{}@github.com/{}/{}", account, key, user_and_repo.0, user_and_repo.1))
        } else {
            Err(Box::new(vcs::VCSError {
                cause: format!("should have github config, got: {}", self.config.vcs)
            }))
        }
    }
}

impl<'a, GIT: (git::GitFeatures<'a>) + (git::GitHubFeatures<'a>)> vcs::VCS<'a> for Github<'a, GIT> {
    fn new(config: &'a config::Config) -> Result<Github<'a, GIT>, Box<dyn Error>> {
        if let config::VCSConfig::Github{ account, key:_, email } = &config.vcs {
            let mut gh = Github::<'a> {
                config: config,
                diff: vec!(),
                git: GIT::new(account, email, config)
            };
            let diff = gh.git.rebase_with_remote_counterpart(&gh.push_url()?, &gh.git.current_branch()?)?;
            gh.diff = diff.split('\n').map(|e| e.to_string()).collect();
            return Ok(gh)
        } 
        return Err(Box::new(vcs::VCSError {
            cause: format!("should have github config but {}", config.vcs)
        }))
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
