use serde::{Deserialize, Serialize};

use crate::config;

#[derive(Serialize, Deserialize)]
pub struct ReleaseTarget {
    tag: Option<bool>,
    patterns: Vec<config::Value>
}
impl ReleaseTarget {
    pub fn paths<'a>(&'a self) -> &'a Vec<config::Value> {
        return &self.patterns
    }
    pub fn is_branch(&self) -> bool {
        return !self.is_tag()
    }
    pub fn is_tag(&self) -> bool {
        return self.tag.unwrap_or(false)
    }
}
