use std::collections::HashMap;
use std::error::Error;
use std::path;
use std::fs;

use maplit::hashmap;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Release {
    pub paths: Option<HashMap<String, String>>,
    pub versions: HashMap<String, u32>,
}

#[derive(Serialize, Deserialize)]
pub struct Endpoints {
    pub prefix: String,
    pub paths: HashMap<String, String>,
    pub releases: HashMap<String, Release>,
}

impl Endpoints {
    pub fn new(prefix: &str) -> Endpoints {
        Endpoints {
            prefix: prefix.to_string(),
            paths: hashmap!{},
            releases: hashmap!{
                "prev".to_string() => Release {
                    paths: None,
                    versions: hashmap!{},
                },
                "curr".to_string() => Release {
                    paths: None,
                    versions: hashmap!{},
                },
                "next".to_string() => Release {
                    paths: None,
                    versions: hashmap!{},
                }
            },
        }
    }
    pub fn load<P: AsRef<path::Path>>(path: P) -> Result<Endpoints, Box<dyn Error>> {
        match fs::read_to_string(path) {
            Ok(content) => Ok(toml::from_str::<Endpoints>(&content)?),
            Err(err) => Err(Box::new(err))
        }        
    }
    pub fn save<P: AsRef<path::Path>>(&self, path: P) -> Result<(), Box<dyn Error>> {
        let as_text = toml::to_string_pretty(&self)?;
        fs::write(&path, &as_text)?;
        Ok(())
    }
    pub fn modify<P: AsRef<path::Path>, F, R: Sized>(
        path: P, f: F
    ) -> Result<R, Box<dyn Error>> 
    where F: Fn(&mut Endpoints) -> Result<R, Box<dyn Error>> {
        let mut ep = Self::load(&path)?;
        let r = f(&mut ep)?;
        ep.save(&path)?;
        Ok(r)
    }
}
