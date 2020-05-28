use std::error::Error;
use std::fmt;

use super::config;

pub trait CI<'a> {
    fn new(config: &'a config::Config) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn run_job(&self, job_name: &str) -> Result<String, Box<dyn Error>>;
    fn wait_job(&self, job_id: &str) -> Result<(), Box<dyn Error>>;
    fn wait_job_by_name(&self, job_id: &str) -> Result<(), Box<dyn Error>>;
    fn changed(&self, patterns: &Vec<&str>) -> bool;
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
