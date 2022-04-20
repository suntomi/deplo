use std::collections::{HashMap};
use std::error::Error;
use std::fmt;

use crate::config;

pub trait Accessor {
    fn var(
        &self, key: &str
    ) -> Option<String>;
    fn vars(
        &self
    ) -> HashMap<String, String>;
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

mod dotenv;

// factorys
fn factory_by<'a, T: Accessor + Factory + 'a>(
    secret: &config::secret::Secret
) -> Result<Box<dyn Accessor + 'a>, Box<dyn Error>> {
    let cmd = T::new(secret)?;
    return Ok(Box::new(cmd) as Box<dyn Accessor + 'a>);
}

pub fn factory<'a>(
    secret: &config::secret::Secret
) -> Result<Box<dyn Accessor + 'a>, Box<dyn Error>> {
    match secret {
        config::secret::Secret::Dotenv {..} => {
            return factory_by::<dotenv::Dotenv>(secret);
        },
        _ => panic!("unsupported secret type")
    };
}

struct Nop {}
impl Accessor for Nop {
    fn var(&self, _key: &str) -> Option<String> {
        panic!("nop secret driver should not be called");
    }
    fn vars(&self) -> HashMap<String, String> {
        panic!("nop secret driver should not be called");
    }
}
pub fn nop() -> Box<dyn Accessor> {
    Box::new(Nop{})
}
