use std::error::Error;
use std::collections::HashMap;
use std::fmt;

use glob::Pattern;
use regex::Regex;
use serde_json::{Value as JsonValue};

use crate::config;
use crate::module;

pub enum DiffMatcher {
    Glob(Vec<String>),
    Regex(Vec<String>)
}

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum RefType {
    Branch,
    Remote,
    Tag,
    Pull,
    Commit,
}
impl fmt::Display for RefType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Branch => write!(f, "branch"),
            Self::Remote => write!(f, "remote"),
            Self::Tag => write!(f, "tag"),
            Self::Pull => write!(f, "pull"),
            Self::Commit => write!(f, "commit"),
        }
    }
}

pub trait VCS {
    fn new(config: &config::Container) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn release_target(&self) -> Option<String>;
    fn current_ref(&self) -> Result<(RefType, String), Box<dyn Error>>;
    fn delete_branch(&self, ref_type: RefType, ref_path: &str) -> Result<(), Box<dyn Error>>;
    fn fetch_branch(&self, branch_name: &str) -> Result<(), Box<dyn Error>>;
    fn fetch_object(&self, hash: &str, ref_name: &str) -> Result<(), Box<dyn Error>>;
    fn squash_branch(&self, n: usize) -> Result<(), Box<dyn Error>>;
    fn commit_hash(&self, expr: Option<&str>) -> Result<String, Box<dyn Error>>;
    fn checkout(&self, commit: &str, branch_name: Option<&str>) -> Result<(), Box<dyn Error>>;
    fn repository_root(&self) -> Result<String, Box<dyn Error>>;
    fn rebase_with_remote_counterpart(&self, branch: &str) -> Result<(), Box<dyn Error>>;
    fn pick_ref(&self, target: &str) -> Result<(), Box<dyn Error>>;
    fn push_branch(
        &self, local_ref: &str, remote_branch: &str, option: &HashMap<&str, &str>
    ) -> Result<(), Box<dyn Error>>;
    fn pr(
        &self, title: &str, head_branch: &str, base_branch: &str, option: &HashMap<&str, &str>
    ) -> Result<(), Box<dyn Error>>;
    fn pr_url_from_env(&self) -> Result<Option<String>, Box<dyn Error>>;
    fn user_and_repo(&self) -> Result<(String, String), Box<dyn Error>>;
    fn release(
        &self, target_ref: (&str, bool), opts: &JsonValue
    ) -> Result<String, Box<dyn Error>>;
    fn release_assets(
        &self, target_ref: (&str, bool), asset_file_path: &str, opts: &JsonValue
    ) -> Result<String, Box<dyn Error>>;
    fn make_diff(&self) -> Result<String, Box<dyn Error>>;
    fn init_diff(&mut self, diff: String) -> Result<(), Box<dyn Error>>;
    fn push_diff(
        &self, remote_branch: &str, msg: &str, patterns: &Vec<&str>, option: &HashMap<&str, &str>
    ) -> Result<bool, Box<dyn Error>>;
    fn diff<'b>(&'b self) -> &'b Vec<String>;

    // derived interfaces
    fn pick_fetched_head(&self) -> Result<(), Box<dyn Error>> { self.pick_ref("FETCH_HEAD") }
    fn checkout_previous(&self) -> Result<(), Box<dyn Error>> { self.checkout("-", None) }
    fn changed<'b>(&'b self, matcher: &DiffMatcher) -> bool {
        let difflines = self.diff();
        if difflines.len() == 1 && difflines[0] == "*" {
            // this specifal pattern indicates everything changed
            return true;
        }
        match matcher {
            DiffMatcher::Glob(patterns) => {
                for pattern in patterns {
                    match Pattern::new(pattern){
                        Ok(gp) => for diff in difflines {
                            if gp.matches(diff) {
                                return true
                            }
                        },
                        Err(err) => {
                            panic!("glob pattern[{}] is invalid expression err:{:?}", pattern, err);
                        }
                    }
                }
            },
            DiffMatcher::Regex(patterns) => {
                for pattern in patterns {
                    match Regex::new(pattern) {
                        Ok(re) => for diff in difflines {
                            if re.is_match(diff) {
                                return true
                            }
                        },
                        Err(err) => {
                            panic!("regex pattern[{}] is invalid expression err:{:?}", pattern, err);
                        }
                    }
                }
            }
        }
        false
    }
}
#[derive(Clone)]
pub struct ModuleDescription;
impl module::Description for ModuleDescription {
    fn ty() -> config::module::Type { return config::module::Type::VCS; }
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
        config::vcs::Account::Github {..} => {
            return factory_by::<github::Github>(config);
        },
        _ => return Err(Box::new(VCSError {
            cause: format!("add factory matching pattern for [{}]", config.borrow().vcs)
        }))
    };
}
