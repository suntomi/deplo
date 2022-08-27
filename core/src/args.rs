use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::result::Result;

use serde_json::{Value as JsonValue, Map as JsonMap};

use crate::util::{str_to_json};

pub mod clap;

pub trait Args : Sized {
    fn create() -> Result<Self, Box<dyn Error>>;
    fn subcommand(&self) -> Option<(&str, Self)>;
    fn occurence_of(&self, name: &str) -> u64;
    fn values_of(&self, name: &str) -> Option<Vec<&str>>;
    fn command_path(&self) -> &Vec<&str>;
    fn value_of(&self, name: &str) -> Option<&str> {
        match self.values_of(name) {
            Some(v) => if v.len() > 0 { Some(v[0]) } else { None },
            None => None
        }
    }
    fn value_or_die(&self, name: &str) -> &str {
        self.value_of(name).expect(&format!("missing cli arg '{}'", name))
    }
    fn json_value_of(&self, name: &str) -> Result<JsonValue, Box<dyn Error>> {
        let mut map = JsonMap::new();  
        match self.values_of(name) {
            Some(v) => {
                for value in v {
                    let mut parts = value.splitn(2, '=');
                    let key = parts.next().expect("value cli arg should have the form of $key=$value");
                    let value = parts.next().unwrap_or("true");
                    map.insert(key.to_string(), str_to_json(value));
                }
            },
            None => {}
        };
        Ok(serde_json::Value::Object(map))
    }
    fn map_of(&self, key: &str) -> HashMap<String, String> {
        let mut envs = HashMap::<String, String>::new();
        match self.values_of(key) {
            Some(es) => for e in es {
                let mut kv = e.split("=");
                let k = kv.next().expect("env arg should be in key=value or key format");
                let v = kv.next().unwrap_or("");
                envs.insert(k.to_string(), v.to_string());
            },
            None => {}
        };
        envs
    }    
    fn error(&self, msg: &str) -> Box<ArgsError> {
        Box::new(ArgsError {
            command_path: self.command_path().join(" "),
            cause: msg.to_string()
        })
    }
}

pub type Default<'a> = clap::Clap<'a>;
    
#[derive(Debug)]
pub struct ArgsError {
    pub command_path: String, 
    pub cause: String
}
impl fmt::Display for ArgsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.command_path, self.cause)
    }
}
impl Error for ArgsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

pub fn create<'a>() -> Result<Default<'a>, Box<dyn Error>> {
    return Default::<'a>::create();
}