use std::collections::{HashMap};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::config;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Account {
    #[serde(rename = "ghaction")]
    GhAction {
        account: config::Value, // if kind is 'app', github app id. for 'user', github account name 
        key: config::Value, // if kind is 'app', github app private key. for 'user', github personal access token of account
        kind: Option<config::Value>, // either string value of 'user' or 'app'
    },
    #[serde(rename = "circleci")]
    CircleCI {
        key: config::Value,
    },
    #[serde(rename = "module")]
    Module(config::module::ConfigFor<crate::ci::ModuleDescription>)
}
impl Account {
    pub fn type_matched(&self, t: &str) -> bool {
        match self {
            Self::Module(c) => return c.value(|v| v.uses.to_string().starts_with(t)),
            _ => {}
        }
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
            Self::Module(c) => c.value(|v| write!(f, "module {}", v.uses.to_string())),
        }
    }    
}

#[derive(Serialize, Deserialize)]
pub struct Accounts(HashMap<String, Account>);
impl Accounts {
    pub fn as_map(&self) -> &HashMap<String, Account> {
        &self.0
    }
    pub fn default(&self) -> &Account {
        self.as_map().get("default").expect("missing default account")
    }
    pub fn is_main(&self, ty: &str) -> bool {
        let d = self.default();
        d.type_matched(ty)
    }
    pub fn get<'a>(&'a self, name: &str) -> Option<&'a Account> {
        self.0.get(name)
    }
}
