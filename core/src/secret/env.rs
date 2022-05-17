use std::error::Error;
use std::result::Result;

use crate::config;
use crate::secret;
use crate::util::{escalate};

pub struct Env {
    pub key: String,
    pub val: String,
}

impl secret::Factory for Env {
    fn new(
        secret_config: &config::secret::Secret
    ) -> Result<Self, Box<dyn Error>> {
        return Ok(match secret_config {
            config::secret::Secret::Env { key } => Self{
                key: key.clone(), val: match std::env::var(&key) {
                    Ok(val) => val,
                    Err(_) => return escalate!(Box::new(secret::SecretError{
                        cause: format!("env var {} not found", key)
                    }))
                }
            },
            _ => panic!("unexpected secret type")
        });
    }
}
impl secret::Accessor for Env {
    fn var(&self) -> Result<Option<String>, Box<dyn Error>> {
        Ok(Some(self.val.clone()))
    }
}
impl secret::Secret for Env {
}
