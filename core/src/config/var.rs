use std::collections::{HashMap};
use std::error::Error;

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
    pub vars: HashMap<String, crate::var::Var>
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
            let s = crate::var::factory(k, runtime_config, var)?;
            vars.insert(k.clone(), s);
        }
        set_ref(vars);
        Ok(())
    }
}


lazy_static! {
    static ref G_VAR_REF: AccessorsRef = {
        AccessorsRef::new()
    };
}
fn set_ref(vars_ref: crate::var::Accessors) {
    G_VAR_REF.set_ref(vars_ref);
}
pub fn var(key: &str) -> Option<String> {
    return G_VAR_REF.var(key);
}
pub fn vars() -> Result<HashMap<String, String>, Box<dyn Error>> {
    return G_VAR_REF.vars();
}
pub fn as_config_values() -> HashMap<String, config::Value> {
    let mut result = hashmap!{};
    for (k, v) in vars().unwrap() {
        result.insert(k.clone(), config::Value::new(&v));
    }
    return result;
}
