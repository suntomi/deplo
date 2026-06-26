use std::collections::{HashMap};
use std::error::Error;
use std::sync::RwLock;

use maplit::hashmap;
use serde::{Deserialize};

use crate::config;
use crate::var::AccessorsRef;

#[derive(Deserialize)]
pub struct SecretConfig {
    #[serde(flatten)]
    pub var: crate::var::Var,
    pub targets: Option<Vec<String>>
}
#[derive(Deserialize)]
pub struct Config {
    pub secrets: HashMap<String, SecretConfig>,
    pub vars: HashMap<String, VarConfig>
}
#[derive(Deserialize)]
pub struct VarConfig {
    #[serde(flatten)]
    pub var: crate::var::Var,
    pub targets: Option<Vec<String>>
}
impl Config {
    pub fn apply_with(
        &self,
        runtime_config: &crate::config::runtime::Config
    ) -> Result<(), Box<dyn Error>> {
        let mut secrets = hashmap!{};
        for (k, secret) in &self.secrets {
            let s = crate::var::factory(k, runtime_config, &secret.var)?;
            secrets.insert(k.clone(), s);
            config::secret::set_targets(k.clone(), secret.targets.clone());
        }
        config::secret::set_ref(secrets);
        let mut vars = hashmap!{};
        for (k, var) in &self.vars {
            let s = crate::var::factory(k, runtime_config, &var.var)?;
            vars.insert(k.clone(), s);
            set_targets(k.clone(), var.targets.clone());
        }
        set_ref(vars);
        Ok(())
    }
}


lazy_static! {
    static ref G_VAR_REF: AccessorsRef = {
        AccessorsRef::new()
    };
    static ref G_VARS_TARGETS: RwLock<HashMap<String, Option<Vec<String>>>> = {
        RwLock::new(HashMap::new())
    };
}
fn set_ref(vars_ref: crate::var::Accessors) {
    G_VAR_REF.set_ref(vars_ref);
}
fn set_targets(k: String, v: Option<Vec<String>>) {
    let mut targets = G_VARS_TARGETS.write().unwrap();
    targets.insert(k, v);
}
pub fn var(key: &str) -> Option<String> {
    return G_VAR_REF.var(key);
}
pub fn vars() -> Result<HashMap<String, String>, Box<dyn Error>> {
    return G_VAR_REF.vars();
}
pub fn targets(key: &str) -> Option<Vec<String>> {
    return match G_VARS_TARGETS.read().unwrap().get(key) {
        None => None,
        Some(v) => v.clone()
    }
}
pub fn as_config_values() -> HashMap<String, config::Value> {
    let mut result = hashmap!{};
    for (k, v) in vars().unwrap() {
        result.insert(k.clone(), config::Value::new(&v));
    }
    return result;
}
