use std::collections::{HashMap};
use std::error::Error;
use std::sync::RwLock;

use maplit::hashmap;

use serde::{Deserialize, Serialize};

type SecretAccessors = HashMap<String, Box<dyn crate::secret::Accessor + Send + Sync>>;

#[derive(Serialize, Deserialize)]
pub enum Secret {
    Nop,
    Env {
        key: String
    },
    File {
        path: String
    },
    Module {
        uses: String,
        with: String
    }    
}

#[derive(Deserialize)]
pub struct Config {
    pub secrets: HashMap<String, Secret>
}
impl Config {
    pub fn apply(&self) -> Result<(), Box<dyn Error>> {
        let mut secrets = hashmap!{};
        for (k, secret) in &self.secrets {
            let s = crate::secret::factory(secret)?;
            secrets.insert(k.clone(), s);
        }
        set_secret_ref(secrets);
        Ok(())
    }
}

lazy_static! {
    static ref G_SECRET_REF: RwLock<SecretAccessors> = {
        RwLock::new(hashmap!{})
    };
}
fn set_secret_ref(secrets_ref: SecretAccessors) {
    *G_SECRET_REF.write().unwrap() = secrets_ref;
}
pub fn var(key: &str) -> Option<String> {
    return match G_SECRET_REF.read().unwrap().get(key) {
        Some(accessor) => accessor.var().unwrap(),
        None => None
    };
}
pub fn vars() -> Result<HashMap<String, String>, Box<dyn Error>> {
    let reader = G_SECRET_REF.read().unwrap();
    let mut result = hashmap!{};
    for (k, v) in &*reader {
        match v.var()? {
            Some(value) => result.insert(k.clone(), value),
            None => continue
        };
    }
    return Ok(result);
}
