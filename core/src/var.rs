use std::collections::{HashMap};
use std::error::Error;
use std::fmt;
use std::sync::RwLock;

use crate::config;

use maplit::hashmap;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum Var {
    Env {
        env: String
    },
    File {
        path: String
    },
    Module {
        uses: String,
        with: String
    }    
}


pub trait Accessor {
    fn var(
        &self
    ) -> Result<Option<String>, Box<dyn Error>>;
}
pub trait Factory {
    fn new(
        name: &str,
        runtime_config: &config::runtime::Config,
        var: &Var
    ) -> Result<Self, Box<dyn Error>> where Self : Sized;
}
pub trait Trait : Accessor + Factory {
}

#[derive(Debug)]
pub struct VarError {
    cause: String
}
impl fmt::Display for VarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl Error for VarError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

mod env;
mod file;

// factorys
fn factory_by<'a, T: Accessor + Factory + Send + Sync + 'a>(
    name: &str,
    runtime_config: &config::runtime::Config,
    var: &Var
) -> Result<Box<dyn Accessor + Send + Sync + 'a>, Box<dyn Error>> {
    let cmd = T::new(name, runtime_config, var)?;
    return Ok(Box::new(cmd) as Box<dyn Accessor + Send + Sync + 'a>);
}

pub fn factory<'a>(
    name: &str,
    runtime_config: &config::runtime::Config,
    var: &Var
) -> Result<Box<dyn Accessor + Send + Sync + 'a>, Box<dyn Error>> {
    match var {
        Var::Env {..} => {
            return factory_by::<env::Env>(name, runtime_config, var);
        },
        Var::File {..} => {
            return factory_by::<file::File>(name, runtime_config, var);
        },
        _ => panic!("unsupported secret type")
    };
}

pub type Accessors = HashMap<String, Box<dyn crate::var::Accessor + Send + Sync>>;
pub struct AccessorsRef(RwLock<Accessors>);
impl AccessorsRef {
    pub fn new() -> Self {
        AccessorsRef(RwLock::new(hashmap!{}))
    }
    pub fn set_ref(&self, refs: crate::var::Accessors) {
        let mut accessor = self.0.write().unwrap();
        *accessor = refs;
    }    
    pub fn var(&self, key: &str) -> Option<String> {
        return match self.0.read().unwrap().get(key) {
            Some(accessor) => accessor.var().unwrap(),
            None => None
        };
    }
    pub fn vars(&self) -> Result<HashMap<String, String>, Box<dyn Error>> {
        let reader = self.0.read().unwrap();
        let mut result = hashmap!{};
        for (k, v) in &*reader {
            match v.var()? {
                Some(value) => result.insert(k.clone(), value),
                None => continue
            };
        }
        return Ok(result);
    }    
}

