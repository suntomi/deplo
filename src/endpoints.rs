use std::collections::HashMap;

use maplit::hashmap;
use serde::{Deserialize, Serialize};

pub type VersionTuple = HashMap<String, u32>;

#[derive(Serialize, Deserialize)]
pub struct Endpoints {
    pub prefix: String,
    pub paths: HashMap<String, String>,
    pub versions: Vec<VersionTuple>,
}

impl Endpoints {
    fn new(prefix: &str) -> Endpoints {
        Endpoints {
            prefix: prefix.to_string(),
            paths: hashmap!{},
            versions: vec!()
        }
    }
}
