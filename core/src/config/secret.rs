use std::collections::{HashMap};
use std::error::Error;

use maplit::hashmap;

use crate::config;
use crate::var::AccessorsRef;

lazy_static! {
    static ref G_SECRET_REF: AccessorsRef = {
        AccessorsRef::new()
    };
}
pub fn set_ref(secrets_ref: crate::var::Accessors) {
    G_SECRET_REF.set_ref(secrets_ref);
}
pub fn var(key: &str) -> Option<String> {
    return G_SECRET_REF.var(key);
}
pub fn vars() -> Result<HashMap<String, String>, Box<dyn Error>> {
    return G_SECRET_REF.vars();
}
pub fn as_config_values() -> HashMap<String, config::Value> {
    let mut result = hashmap!{};
    for (k, _v) in vars().unwrap() {
        result.insert(k.clone(), config::Value::new_secret(&k));
    }
    return result;
}
