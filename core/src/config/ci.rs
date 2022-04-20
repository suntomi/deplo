use std::collections::{HashMap};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::config;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Account {
    #[serde(rename = "ghaction")]
    GhAction {
        account: config::Value,
        key: config::Value,
    },
    #[serde(rename = "circleci")]
    CircleCI {
        key: config::Value,
    },
    #[serde(rename = "module")]
    Module(config::module::ConfigFor<crate::ci::Manifest>)
}
impl Account {
    pub fn type_matched(&self, t: &str) -> bool {
        return t == self.type_as_str()
    }
    pub fn type_as_str(&self) -> &'static str {
        match self {
            Self::GhAction{..} => "GhAction",
            Self::CircleCI{..} => "CircleCI",
            Self::Module{..} => "Module",
        }
    } 
}
impl fmt::Display for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GhAction{..} => write!(f, "ghaction"),
            Self::CircleCI{..} => write!(f, "circleci"),
            Self::Module(c) => write!(f, "module {}", c.value().uses),
        }
    }    
}
