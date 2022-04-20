use std::collections::{HashMap};
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::config;
use crate::module;

#[derive(Serialize, Deserialize)]
pub struct RemoteJob {
    pub name: String,
    pub commit: Option<String>,
    pub command: Option<String>,
    pub envs: HashMap<String, String>,
    pub verbosity: u64,
    pub release_target: Option<String>,
    pub workflow: Option<String>,
}

pub enum OutputKind {
    System,
    User,
}
impl OutputKind {
    fn to_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
        }
    }
}

pub trait CI : module::Module {
    fn new(
        config: &config::Container, account_name: &str
    ) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn kick(&self) -> Result<(), Box<dyn Error>>;
    fn overwrite_commit(&self, commit: &str) -> Result<String, Box<dyn Error>>;
    fn pr_url_from_env(&self) -> Result<Option<String>, Box<dyn Error>>;
    fn mark_job_executed(&self, job_name: &str) -> Result<Option<String>, Box<dyn Error>>;
    fn mark_need_cleanup(&self, job_name: &str) -> Result<(), Box<dyn Error>>;
    fn run_job(&self, job: &RemoteJob) -> Result<String, Box<dyn Error>>;
    fn check_job_finished(&self, job_id: &str) -> Result<Option<String>, Box<dyn Error>>;
    fn set_secret(&self, key: &str, val: &str) -> Result<(), Box<dyn Error>>;
    fn list_secret_name(&self) -> Result<Vec<String>, Box<dyn Error>>;
    fn job_env(&self) -> HashMap<&str, String>;
    fn process_env(&self, local: bool) -> Result<HashMap<&str, String>, Box<dyn Error>>;
    fn dispatched_remote_job(&self) -> Result<Option<RemoteJob>, Box<dyn Error>>;
    fn set_job_output(&self, job_name: &str, kind: OutputKind, outputs: HashMap<&str, &str>) -> Result<(), Box<dyn Error>>;
    fn job_output(&self, job_name: &str, kind: OutputKind, key: &str) -> Result<Option<String>, Box<dyn Error>>;
}
pub struct Manifest;
impl module::Manifest for Manifest {
    fn ty() -> config::module::Type { return config::module::Type::Ci; }
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

pub mod ghaction;
pub mod circleci;

// factorys
fn factory_by<'a, T: CI + 'a>(
    config: &config::Container,
    account_name: &str
) -> Result<Box<dyn CI + 'a>, Box<dyn Error>> {
    let cmd = T::new(config, account_name)?;
    return Ok(Box::new(cmd) as Box<dyn CI + 'a>);
}

pub fn factory<'a>(
    config: &config::Container,
    account_name: &str
) -> Result<Box<dyn CI + 'a>, Box<dyn Error>> {
    match &config.borrow().ci.get(account_name).unwrap() {
        config::ci::Account::GhAction {..} => {
            return factory_by::<ghaction::GhAction>(config, account_name);
        },
        config::ci::Account::CircleCI {..} => {
            return factory_by::<circleci::CircleCI>(config, account_name);
        }
    };
}