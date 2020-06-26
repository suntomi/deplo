use std::error::Error;
use std::fmt;

use regex::Regex;

use super::config;

pub trait CI<'a> {
    fn new(config: &'a config::Config) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn init(&self) -> Result<(), Box<dyn Error>>;
    fn pull_request_url(&self) -> Result<Option<String>, Box<dyn Error>>;
    fn run_job(&self, job_name: &str) -> Result<String, Box<dyn Error>>;
    fn wait_job(&self, job_id: &str) -> Result<(), Box<dyn Error>>;
    fn wait_job_by_name(&self, job_id: &str) -> Result<(), Box<dyn Error>>;
    fn diff<'b>(&'b self) -> &'b Vec<String>;
    fn set_secret(&self, key: &str, val: &str) -> Result<(), Box<dyn Error>>;
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
pub struct CIError {
    cause: String
}
impl fmt::Display for CIError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl Error for CIError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

// subcommands
pub mod ghaction;
pub mod circle;

// factorys
fn factory_by<'a, T: CI<'a> + 'a>(
    config: &'a config::Config
) -> Result<Box<dyn CI<'a> + 'a>, Box<dyn Error>> {
    let cmd = T::new(config).unwrap();
    return Ok(Box::new(cmd) as Box<dyn CI<'a> + 'a>);
}

pub fn factory<'a>(
    config: &'a config::Config
) -> Result<Box<dyn CI<'a> + 'a>, Box<dyn Error>> {
    match &config.ci {
        config::CIConfig::GhAction { account:_, key:_ } => {
            return factory_by::<ghaction::GhAction>(config);
        },
        config::CIConfig::Circle { key:_ } => {
            return factory_by::<circle::Circle>(config);
        }
    };
}
