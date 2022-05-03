use std::collections::{HashMap};
use std::sync::RwLock;

use maplit::hashmap;
use serde::{Deserialize, de::DeserializeOwned, Serialize, Deserializer};

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
#[derive(Serialize, Deserialize)]
pub struct EmptyExtension {}
#[derive(Serialize)]
pub struct ConfigFor<T: module::Manifest, E: DeserializeOwned = EmptyExtension> {
    index: usize,
    ext: E,
    anchor: std::marker::PhantomData<T>,
}
impl<'de, T: module::Manifest, E: DeserializeOwned> ConfigFor<T,E> {
    pub fn value(&self) -> &'static Config {
        &config_for::<T>()[self.index]
    }
    pub fn ext(&self) -> &E {
        &self.ext
    }
}
impl<'de, T: module::Manifest, E: DeserializeOwned> Deserialize<'de> for ConfigFor<T,E> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        let v = Config::deserialize(deserializer)?;
        let e = E::deserialize(deserializer)?;
        set_config_for::<T>(v);
        Ok(ConfigFor::<T,E> { 
            index: config_for::<T>().len() - 1,
            ext: e,
            anchor: std::marker::PhantomData
        })
    }
}
pub fn set_config_for<T: module::Manifest>(config: Config) {
    G_MODULE_CONFIG_REF.write().unwrap().entry(T::ty()).or_insert(vec![]).push(config);
}
pub fn config_for< T: module::Manifest>() -> &'static Vec<Config> {
    return G_MODULE_CONFIG_REF.read().unwrap().get(&T::ty()).unwrap_or(&vec![]);
}
