use std::collections::{HashMap};

use maplit::hashmap;
use serde::{Deserialize, Serialize};

use crate::config;

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
    Module(config::module::ConfigFor<crate::workflow::Module>)
}
#[derive(Deserialize)]
struct CronFilter {
    pub schedule: String
}
#[derive(Deserialize)]
struct RepositortFilter {
    pub trigger: String,
    pub action: Option<String>
}
pub struct Match {
    pub workflow: String,
    pub contexts: HashMap<String, String>
}

impl Workflow {
    fn matches(&self, event: &str, params: &str) -> Option<Match> {
        match &self {
            Self::Deploy if event == "deploy" => {
                Some(Match {
                    workflow: event.to_string(),
                    // deploy and integrate matches changed with its changeset
                    contexts: hashmap!{},
                })
            },
            Self::Integrate if event == "integrate" => {
                Some(Match {
                    workflow: event.to_string(),
                    // deploy and integrate matches changed with its changeset
                    contexts: hashmap!{},
                })
            },
            Self::Cron{schedules} if event == "cron" => {
                let filter = serde_json::from_str::<CronFilter>(params).unwrap();
                for (schedule, cron_pattern) in schedules {
                    if cron_pattern == &filter.schedule {
                        return Some(Match {
                            workflow: event.to_string(),
                            contexts: hashmap!{
                                "schedule".to_string() => schedule.to_string()
                            }
                        })
                    }
                }
                None
            },
            Self::Repository{events} if event == "repository" => {
                let filter = serde_json::from_str::<RepositortFilter>(params).unwrap();
                let key = {
                    let mut v = vec![filter.trigger];
                    if let Some(action) = filter.action {
                        v.push(action)
                    }
                    v.join(".")
                };
                for (event, triggers) in events {
                    if triggers.iter().find(|t| t == &key).is_some() {
                        return Some(Match {
                            workflow: event.to_string(),
                            contexts: hashmap!{
                                "event".to_string() => event.to_string(),
                            },
                        })
                    }
                }
                None
            },
            Self::Module(c) if event == "module" => {
                c.value(|v| {
                    panic!("TODO: implement module workflow with {}, {:?}", v.uses, v.with)
                })
            },
            _ => None
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Workflows(HashMap<String, Workflow>);
impl Workflows {
    pub fn as_map(&self) -> &HashMap<String, Workflow> {
        &self.0
    }
    pub fn get<'a>(&'a self, name: &str) -> Option<&'a Workflow> {
        self.0.get(name)
    }
}
