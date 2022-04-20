use std::collections::{HashMap};
use std::error::Error;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Secret {
    #[serde(rename = "nop")]
    Nop,
    #[serde(rename = "dotenv")]
    Dotenv {
        path: Option<String>,
        keys: Vec<String>
    },
    #[serde(rename = "module")]
    Module { //eg. aws ssm
        uses: String,
        with: Option<HashMap<String, String>>
    },
}

#[derive(Deserialize)]
pub struct Config {
    pub secrets: Secret
}
impl Config {
    pub fn apply(&self) -> Result<(), Box<dyn Error>> {
        let s = crate::secret::factory(&self.secrets)?;
        set_secret_ref(s);
        Ok(())
    }
}

lazy_static! {
    static ref G_SECRET_REF: RwLock<Box<dyn crate::secret::Accessor>> = {
        RwLock::new(crate::secret::nop())
    };
}
fn set_secret_ref(secret_ref: Box<dyn crate::secret::Accessor>) {
    *G_SECRET_REF.write().unwrap() = secret_ref;
}
pub fn var(key: &str) -> Option<String> {
    return G_SECRET_REF.read().unwrap().var(key);
}
pub fn vars() -> HashMap<String, String> {
    return G_SECRET_REF.read().unwrap().vars();
}
