use std::error::Error;
use std::collections::HashMap;
use std::fmt;

use regex::Regex;

use super::config;
use crate::module;

pub trait VCS : module::Module {
    fn new(config: &config::Container) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn release_target(&self) -> Option<String>;
    fn current_branch(&self) -> Result<String, Box<dyn Error>>;
    fn commit_hash(&self) -> Result<String, Box<dyn Error>>;
    fn repository_root(&self) -> Result<String, Box<dyn Error>>;
    fn rebase_with_remote_counterpart(&self, branch: &str) -> Result<String, Box<dyn Error>>;
    fn push(
        &self, remote_branch: &str, msg: &str, patterns: &Vec<&str>, option: &HashMap<&str, &str>
    ) -> Result<bool, Box<dyn Error>>;
    fn pr(
        &self, title: &str, head_branch: &str, base_branch: &str, option: &HashMap<&str, &str>
    ) -> Result<(), Box<dyn Error>>;
    fn user_and_repo(&self) -> Result<(String, String), Box<dyn Error>>;
    fn diff<'b>(&'b self) -> &'b Vec<String>;
    fn changed<'b>(&'b self, patterns: &Vec<&str>) -> bool {
        let difflines = self.diff();
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
    let cmd = T::new(config).unwrap();
    return Ok(Box::new(cmd) as Box<dyn VCS + 'a>);
}

pub fn factory<'a>(
    config: &config::Container
) -> Result<Box<dyn VCS + 'a>, Box<dyn Error>> {
    match &config.borrow().vcs {
        config::VCSConfig::Github { email:_,  account:_, key:_ } => {
            return factory_by::<github::Github>(config);
        },
        _ => return Err(Box::new(VCSError {
            cause: format!("add factory matching pattern for [{}]", config.borrow().vcs)
        }))
    };
}
