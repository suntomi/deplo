use std::collections::{HashMap};
use std::fmt;

use serde::{Deserialize, Serialize};


use crate::config;

#[derive(Serialize, Deserialize, Clone)]
pub struct WorkflowExtension {
    pub release_target: Option<String>,
}
#[derive(Serialize, Deserialize)]
pub enum InputValueType {
    #[serde(rename = "number")]
    Number,
    #[serde(rename = "string")]
    String,
    #[serde(rename = "bool")]
    Bool,
    #[serde(rename = "float")]
    Float,
}
#[derive(Serialize, Deserialize)]
pub enum InputCollectionType {
    #[serde(rename = "list")]
    List,
    #[serde(rename = "map")]
    Map,
}
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum InputSchemaClass {
    Collection{
        #[serde(rename = "type")]
        ty: InputCollectionType,
        schema: Box<InputSchema>
    },
    Value{
        #[serde(rename = "type")]
        ty: InputValueType
    },
    Tuple{schema: Vec<Box<InputSchema>>},
    Object{schema: HashMap<String, Box<InputSchema>>}
}
#[derive(Serialize, Deserialize)]
pub struct InputSchema {
    #[serde(flatten)]
    pub class: InputSchemaClass,
    pub required: Option<bool>,
    pub description: Option<String>
}
impl InputSchema {
    pub fn verify(&self, _inputs: &serde_json::Value) {
        log::warn!("TODO: actually verify input and panics when not valid")
    }
}
#[derive(Serialize, Deserialize)]
pub struct InputSchemaSet(HashMap<String, Box<InputSchema>>);
impl InputSchemaSet {
    pub fn as_map(&self) -> &HashMap<String, Box<InputSchema>> {
        &self.0
    }
    pub fn verify(&self, inputs: &serde_json::Value) {
        if !inputs.is_object() {
            panic!("input must be object but {}", serde_json::to_string(inputs).unwrap())
        }
        for (name, schema) in &self.0 {
            match inputs.as_object().unwrap().get(name) {
                Some(v) => schema.verify(v),
                None => if schema.required.unwrap_or(false) {
                    panic!(
                        "input must be contains key `{}` but not: {}",
                        name, serde_json::to_string(inputs).unwrap()
                    )                    
                }
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum Workflow {
    Deploy,
    Integrate,
    Cron {
        schedules: HashMap<String, config::Value>
    },
    Repository {
        events: HashMap<String, Vec<config::Value>>
    },
    Dispatch {
        manual: Option<bool>,
        inputs: InputSchemaSet
    },
    Module(config::module::ConfigFor<crate::workflow::ModuleDescription, WorkflowExtension>),
}
impl fmt::Display for Workflow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Deploy => write!(f, "deploy"),
            Self::Integrate => write!(f, "integrate"),
            Self::Cron{schedules} => write!(f, "cron:{:?}", schedules),
            Self::Repository{events} => write!(f, "repository:{:?}", events),
            Self::Dispatch{manual,..} => write!(f, "dispatch:{:?}", manual),
            Self::Module(m) => m.value(|v| write!(f, "{}({:?})", v.uses.to_string(), v.with))
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Workflows(HashMap<String, Workflow>);
impl Workflows {
    fn insert_or_die(&mut self, name: &str, value: Workflow) {
        if self.0.contains_key(name) {
            panic!("{} is reserved workflow name", name);
        }
        self.0.insert(name.to_string(), value);
    }
    pub fn setup(&mut self) {
        self.insert_or_die("deploy", Workflow::Deploy);
        self.insert_or_die("integrate", Workflow::Integrate);
        let mut cron_found = None;
        let mut repo_found = None;
        for (name, wf) in &self.0 {
            match wf {
                Workflow::Cron{..} => if cron_found.is_some() {
                    panic!(
                        "{} is cron workflow but you cannot define it twice (already have {}) for now",
                        name, cron_found.unwrap()
                    );
                } else {
                    cron_found = Some(name);
                },
                Workflow::Repository{..} => if repo_found.is_some() {
                    panic!(
                        "{} is repository event workflow but you cannot define it twice (already have {}) for now",
                        name, repo_found.unwrap()
                    );
                } else {
                    repo_found = Some(name);
                },
                _ => {}
            }
        }
    }
    pub fn as_map(&self) -> &HashMap<String, Workflow> {
        &self.0
    }
    pub fn get<'a>(&'a self, name: &str) -> Option<&'a Workflow> {
        self.0.get(name)
    }
}
