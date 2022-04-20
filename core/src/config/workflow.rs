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
    Module(config::module::ConfigFor<crate::workflow::Manifest>)
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
pub struct MatchedWorkflow {
    pub workflow: String,
    pub workflow_params: HashMap<String, String>
}

impl Workflow {
    fn matches(&self, event: &str, params: &str) -> Option<MatchedWorkflow> {
        match &self {
            Self::Deploy if event == "deploy" => {
                Some(MatchedWorkflow {
                    workflow: event.to_string(),
                    // deploy and integrate matches changed with its changeset
                    workflow_params: hashmap!{},
                })
            },
            Self::Integrate if event == "integrate" => {
                Some(MatchedWorkflow {
                    workflow: event.to_string(),
                    // deploy and integrate matches changed with its changeset
                    workflow_params: hashmap!{},
                })
            },
            Self::Cron{schedules} if event == "cron" => {
                let filter = serde_json::from_str::<CronFilter>(params).unwrap();
                for (schedule, cron_pattern) in schedules {
                    if cron_pattern == &filter.schedule {
                        return Some(MatchedWorkflow {
                            workflow: event.to_string(),
                            workflow_params: hashmap!{
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
                        return Some(MatchedWorkflow {
                            workflow: event.to_string(),
                            workflow_params: hashmap!{
                                "event".to_string() => event.to_string(),
                            },
                        })
                    }
                }
                None
            },
            Self::Module(c) if event == "module" => {
                let v = c.value();
                panic!("TODO: implement module workflow with {}, {:?}", v.uses, v.with)
            },
            _ => None
        }
    }
}