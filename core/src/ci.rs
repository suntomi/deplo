use std::collections::{HashMap};
use std::error::Error;
use std::fmt;

use crate::config;
use crate::module;
use crate::util::{strhash};

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
    pub fn env_name_for_job(&self, job_name: &str) -> String {
        format!(
            "DEPLO_JOB_{}_OUTPUT_{}",
            self.to_str().to_uppercase(), job_name.replace("-", "_").to_uppercase()
        )
    }

}
pub enum WorkflowTrigger {
    /// payload from CI service (github, circleci, etc...)
    EventPayload(String),
}
impl WorkflowTrigger {
    pub fn to_string(&self) -> String {
        match self {
            Self::EventPayload(payload) => format!("EventPayload({})", payload),
        }
    }
}

pub trait CheckoutOption {
    fn opt_str(&self) -> Vec<String>;
    fn to_yaml_config(value: &config::Value) -> String;
    fn hash(&self) -> String {
        let src = self.opt_str().join(",");
        strhash(&src)
    }
}

pub trait CI {
    fn new(
        config: &config::Container, account_name: &str
    ) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn runs_on_service(&self) -> bool;
    fn generate_config(&self, reinit: bool) -> Result<(), Box<dyn Error>>;
    fn overwrite_commit(&self, commit: &str) -> Result<String, Box<dyn Error>>;
    fn pr_url_from_env(&self) -> Result<Option<String>, Box<dyn Error>>;
    fn schedule_job(&self, job_name: &str) -> Result<(), Box<dyn Error>>;
    fn mark_need_cleanup(&self, job_name: &str) -> Result<(), Box<dyn Error>>;
    fn run_job(&self, job_config: &config::runtime::Workflow) -> Result<String, Box<dyn Error>>;
    fn check_job_finished(&self, job_id: &str) -> Result<Option<String>, Box<dyn Error>>;
    fn set_secret(&self, key: &str, val: &str) -> Result<(), Box<dyn Error>>;
    fn list_secret_name(&self) -> Result<Vec<String>, Box<dyn Error>>;
    fn job_env(&self) -> HashMap<String, config::Value>;
    fn process_env(&self) -> Result<HashMap<&str, String>, Box<dyn Error>>;
    fn filter_workflows(
        &self, trigger: Option<WorkflowTrigger>
    ) -> Result<Vec<config::runtime::Workflow>, Box<dyn Error>>;
    fn set_job_output(&self, job_name: &str, kind: OutputKind, outputs: HashMap<&str, &str>) -> Result<(), Box<dyn Error>>;
    fn job_output(&self, job_name: &str, kind: OutputKind, key: &str) -> Result<Option<String>, Box<dyn Error>>;
}
#[derive(Clone)]
pub struct ModuleDescription;
impl module::Description for ModuleDescription {
    fn ty() -> config::module::Type { return config::module::Type::CI; }
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
    match &config.borrow().ci.get(account_name).expect(&format!("ci account {} should defined in Deplo.toml", account_name)) {
        config::ci::Account::GhAction {..} => {
            return factory_by::<ghaction::GhAction>(config, account_name);
        },
        config::ci::Account::CircleCI {..} => {
            return factory_by::<circleci::CircleCI>(config, account_name);
        },
        _ => panic!("unsupported ci account type {}", account_name)
    };
}