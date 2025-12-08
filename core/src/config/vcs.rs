use std::fmt;

use serde::{Deserialize, Serialize};

use crate::config;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Account {
    #[serde(rename = "github")]
    Github {
        email: config::Value,
        account: config::Value,
        key: config::Value
    },
    #[serde(rename = "github_app")]
    GithubApp {
        app_id: config::Value,
        pkey: config::Value
    },
    #[serde(rename = "gitlab")]
    Gitlab {
        email: config::Value,
        account: config::Value,
        key: config::Value
    },
    #[serde(rename = "module")]
    Module(config::module::ConfigFor<crate::vcs::ModuleDescription>)
}
impl fmt::Display for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Github{..} => write!(f, "github"),
            Self::GithubApp{..} => write!(f, "github_app"),
            Self::Gitlab{..} => write!(f, "gitlab"),
            Self::Module(c) => c.value(|v| write!(f, "module {}", v.uses.to_string())),
        }
    }    
}
