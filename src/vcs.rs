use std::error::Error;
use std::fmt;

use super::config;

pub trait VCS<'a> {
    fn new(config: &'a config::Config) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn release_target(&self) -> Option<String>;
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
fn factory_by<'a, T: VCS<'a> + 'a>(
    config: &'a config::Config
) -> Result<Box<dyn VCS<'a> + 'a>, Box<dyn Error>> {
    let cmd = T::new(config).unwrap();
    return Ok(Box::new(cmd) as Box<dyn VCS<'a> + 'a>);
}

pub fn factory<'a>(
    config: &'a config::Config
) -> Result<Box<dyn VCS<'a> + 'a>, Box<dyn Error>> {
    match &config.vcs {
        config::VCSConfig::Github { email:_,  account:_, key:_ } => {
            return factory_by::<github::Github>(config);
        },
        _ => return Err(Box::new(VCSError {
            cause: format!("add factory matching pattern for [{}]", config.vcs)
        }))
    };
}
