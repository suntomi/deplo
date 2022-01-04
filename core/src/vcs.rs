use std::error::Error;
use std::collections::HashMap;
use std::fmt;

use regex::Regex;
use serde_json::{Value as JsonValue};

use super::config;
use crate::module;

#[derive(Eq, PartialEq, Debug)]
pub enum RefType {
    Branch,
    Remote,
    Tag,
    Pull,
    Commit,
}

pub trait VCS : module::Module {
    fn new(config: &config::Container) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn release_target(&self) -> Option<String>;
    fn current_ref(&self) -> Result<(RefType, String), Box<dyn Error>>;
    fn current_branch(&self) -> Result<(String, bool), Box<dyn Error>>;
    fn commit_hash(&self, expr: Option<&str>) -> Result<String, Box<dyn Error>>;
    fn repository_root(&self) -> Result<String, Box<dyn Error>>;
    fn rebase_with_remote_counterpart(&self, branch: &str) -> Result<(), Box<dyn Error>>;
    fn push(
        &self, remote_branch: &str, msg: &str, patterns: &Vec<&str>, option: &HashMap<&str, &str>
    ) -> Result<bool, Box<dyn Error>>;
    fn pr(
        &self, title: &str, head_branch: &str, base_branch: &str, option: &HashMap<&str, &str>
    ) -> Result<(), Box<dyn Error>>;
    fn pr_url_from_current_ref(&self) -> Result<Option<String>, Box<dyn Error>>;
    fn user_and_repo(&self) -> Result<(String, String), Box<dyn Error>>;
    fn release(
        &self, target_ref: (&str, bool), opts: &JsonValue
    ) -> Result<String, Box<dyn Error>>;
    fn release_assets(
        &self, target_ref: (&str, bool), asset_file_path: &str, opts: &JsonValue
    ) -> Result<String, Box<dyn Error>>;
    fn make_diff(&self) -> Result<String, Box<dyn Error>>;
    fn init_diff(&mut self, diff: String) -> Result<(), Box<dyn Error>>; 
    fn diff<'b>(&'b self) -> &'b Vec<String>;
    fn changed<'b>(&'b self, patterns: &Vec<&str>) -> bool {
        let difflines = self.diff();
        if difflines.len() == 1 && difflines[0] == "*" {
            // this specifal pattern indicates everything changed
            return true;
        }
        for pattern in patterns {
            match Regex::new(pattern) {
                Ok(re) => for diff in difflines {
                    if re.is_match(diff) {
                        return true
                    }
                },
                Err(err) => {
                    panic!("pattern[{}] is invalid regular expression err:{:?}", pattern, err);
                }
            }
        }
        false
    }
}

#[derive(Debug)]
pub struct VCSError {
    cause: String
}
impl fmt::Display for VCSError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl Error for VCSError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

// subcommands
pub mod git;
pub mod github;

// factorys
fn factory_by<'a, T: VCS + 'a>(
    config: &config::Container
) -> Result<Box<dyn VCS + 'a>, Box<dyn Error>> {
    let cmd = T::new(config)?;
    return Ok(Box::new(cmd) as Box<dyn VCS + 'a>);
}

pub fn factory<'a>(
    config: &config::Container
) -> Result<Box<dyn VCS + 'a>, Box<dyn Error>> {
    match &config.borrow().vcs {
        config::VCSConfig::Github {..} => {
            return factory_by::<github::Github>(config);
        },
        _ => return Err(Box::new(VCSError {
            cause: format!("add factory matching pattern for [{}]", config.borrow().vcs)
        }))
    };
}
