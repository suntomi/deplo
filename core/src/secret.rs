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
    secret: &config::secret::Secret
) -> Result<Box<dyn Accessor + Send + Sync + 'a>, Box<dyn Error>> {
    let cmd = T::new(secret)?;
    return Ok(Box::new(cmd) as Box<dyn Accessor + Send + Sync + 'a>);
}

pub fn factory<'a>(
    secret: &config::secret::Secret
) -> Result<Box<dyn Accessor + Send + Sync + 'a>, Box<dyn Error>> {
    match secret {
        config::secret::Secret::Env {..} => {
            return factory_by::<env::Env>(secret);
        },
        config::secret::Secret::File {..} => {
            return factory_by::<file::File>(secret);
        },
        _ => panic!("unsupported secret type")
    };
}

struct Nop {}
impl Accessor for Nop {
    fn var(&self) -> Result<Option<String>, Box<dyn Error>> {
        panic!("nop secret driver should not be called");
    }
}
pub fn nop() -> Box<dyn Accessor + Send + Sync> {
    Box::new(Nop{})
}
