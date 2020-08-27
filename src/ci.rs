use std::error::Error;
use std::fmt;

use super::config;

pub trait CI {
    fn new(
        config: &config::Container, account_name: &str
    ) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn init(&self, reinit: bool) -> Result<(), Box<dyn Error>>;
    fn pull_request_url(&self) -> Result<Option<String>, Box<dyn Error>>;
    fn run_job(&self, job_name: &str) -> Result<String, Box<dyn Error>>;
    fn wait_job(&self, job_id: &str) -> Result<(), Box<dyn Error>>;
    fn wait_job_by_name(&self, job_id: &str) -> Result<(), Box<dyn Error>>;
    fn set_secret(&self, key: &str, val: &str) -> Result<(), Box<dyn Error>>;
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
fn factory_by<'a, T: CI + 'a>(
    config: &config::Container,
    account_name: &str
) -> Result<Box<dyn CI + 'a>, Box<dyn Error>> {
    let cmd = T::new(config, account_name).unwrap();
    return Ok(Box::new(cmd) as Box<dyn CI + 'a>);
}

pub fn factory<'a>(
    config: &config::Container,
    account_name: &str
) -> Result<Box<dyn CI + 'a>, Box<dyn Error>> {
    match &config.borrow().ci_config(account_name) {
        config::CIConfig::GhAction { account:_, key:_, action:_ } => {
            return factory_by::<ghaction::GhAction>(config, account_name);
        },
        config::CIConfig::Circle { key:_, action:_ } => {
            return factory_by::<circle::Circle>(config, account_name);
        }
    };
}
