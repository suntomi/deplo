use std::collections::{HashMap};
use std::error::Error;
use std::fmt;
use std::sync::RwLock;

use maplit::hashmap;
use serde::{Deserialize, Deserializer, de::DeserializeOwned, Serialize, de::Error as DeserializeError};

use crate::config;
use crate::module;
use crate::util::{escalate};

#[derive(Serialize, Deserialize, Hash, PartialEq, Eq)]
// this annotation and below impl TryFrom<String> are 
// required because EntryPointType is used as HashMap key.
// see https://stackoverflow.com/a/68580953/1982282 for detail
#[serde(try_from = "String")]
pub enum Type {
    #[serde(rename = "ci")]
    CI,
    #[serde(rename = "vcs")]
    VCS,
    #[serde(rename = "step")]
    Step,
    #[serde(rename = "workflow")]
    Workflow,
    #[serde(rename = "jobhook")]
    JobHook,
    #[serde(rename = "secret")]
    Secret,
}
impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self {
            Self::CI => "ci",
            Self::VCS => "vcs",
            Self::Step => "step",
            Self::Workflow => "workflow",
            Self::JobHook => "jobhook",
            Self::Secret => "secret"
        })
    }
}
impl TryFrom<String> for Type {
    type Error = Box<dyn Error>;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.as_str() {
            "ci" => Ok(Self::CI),
            "vcs" => Ok(Self::VCS),
            "step" => Ok(Self::Step),
            "workflow" => Ok(Self::Workflow),
            "jobhook" => Ok(Self::JobHook),
            "secret" => Ok(Self::Secret),
            _ => escalate!(Box::new(crate::config::ConfigError{
                cause: format!("no such module entrypoint: {}", s)
            })),
        }
    }
}

lazy_static! {
    pub static ref G_MODULE_CONFIG_REF: RwLock<HashMap<Type, Vec<Config>>> = {
        RwLock::new(hashmap!{})
    };
    pub static ref G_EMPTY_VEC: Vec<Config> = vec![];
}

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub uses: module::Source,
    pub with: Option<HashMap<String, config::AnyValue>>,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct EmptyExtension {}
#[derive(Serialize, Clone)]
pub struct ConfigFor<T: module::Description + Clone, E: DeserializeOwned + Clone = EmptyExtension> {
    index: usize,
    ext: E,
    anchor: std::marker::PhantomData<T>,
}
impl<'de, T: module::Description + Clone, E: DeserializeOwned + Clone> ConfigFor<T,E> {
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
impl<'de, T: module::Description + Clone, E: DeserializeOwned + Clone> Deserialize<'de> for ConfigFor<T,E> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        let v = config::AnyValue::deserialize(deserializer)?;
        let src = toml::to_string(&v).map_err(D::Error::custom)?;
        let c = match toml::from_str::<Config>(&src) {
            Ok(v) => v,
            Err(e) => {
                let cause = e.to_string();
                log::error!("module config deserialize error:'{}', src='{}'", cause, src);
                return Err(D::Error::custom(cause))
            }
        };
        let e = match toml::from_str::<E>(&src) {
            Ok(v) => v,
            Err(e) => {
                let cause = e.to_string();
                log::error!("module ext deserialize error:'{}', src='{}'", cause, src);
                return Err(D::Error::custom(cause))
            }
        };
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
pub fn set_config_for<T: module::Description>(config: Config) {
    G_MODULE_CONFIG_REF.write().unwrap().entry(T::ty()).or_insert(vec![]).push(config);
}
pub fn config_for<T, V, R, E>(mut visitor: V) -> Result<R, E>
where T: module::Description, V: FnMut(&Vec<Config>) -> Result<R, E> {
    let state = G_MODULE_CONFIG_REF.read().unwrap();
    match state.get(&T::ty()) {
        Some(v) => visitor(v),
        None => visitor(&G_EMPTY_VEC),
    }
}
