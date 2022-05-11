use std::collections::{HashMap};
use std::error::Error;
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
    pub static ref G_EMPTY_VEC: Vec<Config> = vec![];
}

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub uses: config::Value,
    pub with: Option<HashMap<String, config::AnyValue>>,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct EmptyExtension {}
#[derive(Serialize)]
pub struct ConfigFor<T: module::Manifest, E: DeserializeOwned = EmptyExtension> {
    index: usize,
    ext: E,
    anchor: std::marker::PhantomData<T>,
}
impl<'de, T: module::Manifest, E: DeserializeOwned> ConfigFor<T,E> {
    pub fn value<R,V>(&self, mut visitor: V) -> R
    where V: FnMut(&Config) -> R {
        config_for::<T, _, R, ()>(|v| {
            Ok(visitor(&v[self.index]))
        }).unwrap()
    }
    pub fn ext(&self) -> &E {
        &self.ext
    }
}
impl<'de, T: module::Manifest, E: DeserializeOwned + Clone> Deserialize<'de> for ConfigFor<T,E> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        let v = config::AnyValue::deserialize(deserializer)?;
        let src = toml::to_string(&v).unwrap();
        let c = toml::from_str::<Config>(&src).unwrap();
        let e = toml::from_str::<E>(&src).unwrap();
        set_config_for::<T>(c);
        config_for::<T, _, Self, D::Error>(|v| {
            Ok(Self { 
                index: v.len() - 1,
                ext: e.clone(),
                anchor: std::marker::PhantomData
            })
        })
    }
}
pub fn set_config_for<T: module::Manifest>(config: Config) {
    G_MODULE_CONFIG_REF.write().unwrap().entry(T::ty()).or_insert(vec![]).push(config);
}
pub fn config_for<T, V, R, E>(mut visitor: V) -> Result<R, E>
where T: module::Manifest, V: FnMut(&Vec<Config>) -> Result<R, E> {
    let state = G_MODULE_CONFIG_REF.read().unwrap();
    visitor(state.get(&T::ty()).unwrap())
}
