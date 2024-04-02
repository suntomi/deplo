use std::error::Error;
use std::result::Result;

use crate::config;
use crate::var;
use crate::util::{escalate};

pub struct Env {
    pub key: String,
    pub val: String,
}

impl var::Factory for Env {
    fn new(
        _name: &str,
        _runtime_config: &config::runtime::Config,
        var: &var::Var
    ) -> Result<Self, Box<dyn Error>> {
        return Ok(match var {
            var::Var::Env { env } => Self{
                key: env.clone(), val: match std::env::var(&env) {
                    Ok(val) => val,
                    Err(_) => return escalate!(Box::new(var::VarError{
                        cause: format!("env var {} not found", env)
                    }))
                }
            },
            _ => panic!("unexpected secret type")
        });
    }
}
impl var::Accessor for Env {
    fn var(&self) -> Result<Option<String>, Box<dyn Error>> {
        Ok(Some(self.val.clone()))
    }
}
impl var::Trait for Env {
}
