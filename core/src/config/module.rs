use std::collections::{HashMap};
use std::sync::RwLock;

use maplit::hashmap;
use serde::{de, Deserialize, Serialize, Deserializer};

use crate::config;
use crate::module;

#[derive(Eq,PartialEq,Hash)]
pub enum Type {
    Ci,
    Secret,
    Step,
    Vcs,
    Workflow,
}

lazy_static! {
    pub static ref G_MODULE_CONFIG_REF: RwLock<HashMap<Type, Vec<Config>>> = {
        RwLock::new(hashmap!{})
    };
}

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub uses: config::Value,
    pub with: Option<HashMap<String, config::AnyValue>>,
}
#[derive(Serialize)]
pub struct ConfigFor<T: module::Manifest> {
    index: usize,
    anchor: std::marker::PhantomData<T>,
}
impl<T: module::Manifest> ConfigFor<T> {
    pub fn value<'a>(&self) -> &'a Config {
        &config_for::<T>()[self.index]
    }
}
impl<'de, T: module::Manifest> Deserialize<'de> for ConfigFor<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        let v = Config::deserialize(deserializer)?;
        set_config_for::<T>(v);
        Ok(ConfigFor::<T> { 
            index: config_for::<T>().len() - 1,
            anchor: std::marker::PhantomData
        })
    }
}
pub fn set_config_for<T: module::Manifest>(config: Config) {
    G_MODULE_CONFIG_REF.write().unwrap().entry(T::ty()).or_insert(vec![]).push(config);
}
pub fn config_for<'a, T: module::Manifest>() -> &'a Vec<Config> {
    return G_MODULE_CONFIG_REF.read().unwrap().get(&T::ty()).unwrap_or(&vec![]);
}
