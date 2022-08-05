use std::collections::{HashMap};
use std::fmt;

use serde::{Deserialize, Serialize};


use crate::config;

#[derive(Serialize, Deserialize, Clone)]
pub struct WorkflowExtension {
    pub release_target: Option<String>,
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
    Module(config::module::ConfigFor<crate::workflow::Module, WorkflowExtension>)
}
impl fmt::Display for Workflow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Deploy => write!(f, "deploy"),
            Self::Integrate => write!(f, "integrate"),
            Self::Cron{schedules} => write!(f, "cron:{:?}", schedules),
            Self::Repository{events} => write!(f, "repository:{:?}", events),
            Self::Module(m) => m.value(|v| write!(f, "{}({:?})", v.uses, v.with))
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
    }
    pub fn as_map(&self) -> &HashMap<String, Workflow> {
        &self.0
    }
    pub fn get<'a>(&'a self, name: &str) -> Option<&'a Workflow> {
        self.0.get(name)
    }
}
