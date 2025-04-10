use std::collections::{HashMap};
use std::error::Error;
use std::sync::RwLock;

use maplit::hashmap;

use crate::config;
use crate::var::AccessorsRef;

lazy_static! {
    static ref G_SECRET_REF: AccessorsRef = {
        AccessorsRef::new()
    };
    static ref G_SECRET_TARGETS: RwLock<HashMap<String, Option<Vec<String>>>> = {
        RwLock::new(HashMap::new())
    };
}
pub fn set_ref(secrets_ref: crate::var::Accessors) {
    G_SECRET_REF.set_ref(secrets_ref);
}
pub fn set_targets(k: String, v: Option<Vec<String>>) {
    let mut targets = G_SECRET_TARGETS.write().unwrap();
    targets.insert(k, v);
}
pub fn var(key: &str) -> Option<String> {
    return G_SECRET_REF.var(key);
}
pub fn vars() -> Result<HashMap<String, String>, Box<dyn Error>> {
    return G_SECRET_REF.vars();
}
pub fn targets(key: &str) -> Option<Vec<String>> {
    return match G_SECRET_TARGETS.read().unwrap().get(key) {
        None => None,
        Some(v) => v.clone()
    }
}
pub fn as_config_values() -> HashMap<String, config::Value> {
    let mut result = hashmap!{};
    for (k, _v) in vars().unwrap() {
        result.insert(k.clone(), config::Value::new_secret(&k));
    }
    return result;
}
