use std::collections::{HashMap};
use std::error::Error;
use std::fmt;

use crate::config;

pub trait Accessor {
    fn var(
        &self
    ) -> Result<Option<String>, Box<dyn Error>>;
}
pub trait Factory {
    fn new(
        runtime_config: &config::runtime::Config,
        secret_config: &config::secret::Secret
    ) -> Result<Self, Box<dyn Error>> where Self : Sized;
}
pub trait Secret : Accessor + Factory {
}

#[derive(Debug)]
pub struct SecretError {
    cause: String
}
impl fmt::Display for SecretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl Error for SecretError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

mod env;
mod file;

// factorys
fn factory_by<'a, T: Accessor + Factory + Send + Sync + 'a>(
    runtime_config: &config::runtime::Config,
    secret: &config::secret::Secret
) -> Result<Box<dyn Accessor + Send + Sync + 'a>, Box<dyn Error>> {
    let cmd = T::new(runtime_config, secret)?;
    return Ok(Box::new(cmd) as Box<dyn Accessor + Send + Sync + 'a>);
}

pub fn factory<'a>(
    runtime_config: &config::runtime::Config,
    secret: &config::secret::Secret
) -> Result<Box<dyn Accessor + Send + Sync + 'a>, Box<dyn Error>> {
    match secret {
        config::secret::Secret::Env {..} => {
            return factory_by::<env::Env>(runtime_config, secret);
        },
        config::secret::Secret::File {..} => {
            return factory_by::<file::File>(runtime_config, secret);
        },
        _ => panic!("unsupported secret type")
    };
}
