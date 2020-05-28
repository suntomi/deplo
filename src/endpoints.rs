use std::collections::HashMap;
use std::error::Error;
use std::path;
use std::fs;
use std::fmt;

use maplit::hashmap;
use serde::{Deserialize, Serialize};

use crate::config;
use crate::command::service::plan;

#[derive(Serialize, Deserialize, PartialEq, Clone)]
pub enum DeployState {
    Invalid,
    ConfirmCascade,
    BeforeCascade,
    BeforeCleanup,
}
impl fmt::Display for DeployState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid => write!(f, "invalid"),
            Self::ConfirmCascade => write!(f, "confirm_cascade"),
            Self::BeforeCascade => write!(f, "before_cascade"),
            Self::BeforeCleanup => write!(f, "before_cleanup"),
        }
    }
}


#[derive(Serialize, Deserialize, Clone)]
pub struct Release {
    pub paths: Option<HashMap<String, String>>,
    pub versions: HashMap<String, u32>,
}

impl PartialEq for Release {
    fn eq(&self, other: &Self) -> bool {
        self.paths == other.paths
    }
}

#[derive(Serialize, Deserialize)]
pub struct Endpoints {
    pub version: u32,
    pub confirm_deploy: Option<bool>,
    pub clients: Vec<String>,
    pub prefix: String,
    pub default: Option<String>,
    pub paths: HashMap<String, String>,
    pub releases: HashMap<String, Release>,
    pub deploy_state: Option<DeployState>,
}

impl Endpoints {
    pub fn new(prefix: &str) -> Endpoints {
        Endpoints {
            version: 0,
            confirm_deploy: None,
            clients: vec!(),
            prefix: prefix.to_string(),
            default: None,
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
            deploy_state: None,
        }
    }
    pub fn load<P: AsRef<path::Path>>(config: &config::Config, path: P) -> Result<Endpoints, Box<dyn Error>> {
        match fs::read_to_string(path) {
            Ok(content) => {
                let ep = toml::from_str::<Endpoints>(&content)?;
                ep.verify(config)?;
                Ok(ep)
            },
            Err(err) => Err(Box::new(err))
        }        
    }
    pub fn save<P: AsRef<path::Path>>(&self, path: P) -> Result<(), Box<dyn Error>> {
        let as_text = toml::to_string_pretty(&self)?;
        fs::write(&path, &as_text)?;
        Ok(())
    }
    pub fn persist(&self, config: &config::Config) -> Result<(), Box<dyn Error>> {
        self.save(config.endpoints_file_path(Some(self.target())))
    }
    pub fn modify<P: AsRef<path::Path>, F, R: Sized>(
        config: &config::Config, path: P, f: F
    ) -> Result<R, Box<dyn Error>> 
    where F: Fn(&mut Endpoints) -> Result<R, Box<dyn Error>> {
        let mut ep = Self::load(config, &path)?;
        let r = f(&mut ep)?;
        ep.save(&path)?;
        Ok(r)
    }
    pub fn path_will_change(&self, config: &config::Config) -> Result<bool, Box<dyn Error>> {
        let next = self.releases.get("next").unwrap();
        for (key, value) in &next.versions {
            if self.version_changed(key) {
                let plan = plan::Plan::load(config, key)?;
                if plan.has_bluegreen_deployment()? {
                    return Ok(true)
                }
            }
        }
        return Ok(false)
    }
    pub fn cascade_versions(&mut self, release_name: Option<&str>, force: bool) -> Result<(), Box<dyn Error>> {
        let curr = self.releases.get("curr").unwrap().clone();
        let next = self.releases.get("next").unwrap().clone();
        if release_name == None && curr == next {
            log::debug!("already updated");
            return Ok(())
        }
        if release_name == None || release_name == Some("prev") {
            if force || self.should_cascade_current() {
                self.releases.entry("prev".to_string()).and_modify(|e| *e = curr);
            }
        }
        if release_name == None || release_name == Some("curr") {
            self.releases.entry("curr".to_string()).and_modify(|e| *e = next);
        }

        Ok(())
    }
    pub fn version_up(&mut self) -> Result<(), Box<dyn Error>> {
        self.version += 1;
        Ok(())
    }
    pub fn get_version(&self, release_name: &str, service: &str) -> u32 {
        let release = self.releases.get(release_name).unwrap();
        match release.versions.get(service) {
            Some(v) => *v,
            None => 0
        }
    }

    fn verify(&self, config: &config::Config) -> Result<(), Box<dyn Error>> {
        for client in &self.clients {
            let pathbuf = config.services_path().join(format!("{}.toml", client));
            match fs::metadata(&pathbuf) {
                Ok(_) => {},
                Err(err) => return Err(Box::new(config::ConfigError {
                    cause: format!(
                        "{}: service names appeared in clients array \
                        should have corresponding service deployment file({}) \
                        but cause error {:?}",
                        self.target(), pathbuf.to_str().unwrap(), err
                    )
                }))
            }
        }
        Ok(())
    }
    fn target<'a>(&'a self) -> &'a str {
        self.prefix.split(".").collect::<Vec<&str>>()[0]
    }
    fn should_cascade_current(&self) -> bool {
        for client in &self.clients {
            if self.version_changed(client) {
                return true
            }
        }
        return false
    }
    fn version_changed(&self, service: &str) -> bool {
        self.get_version("curr", service) != self.get_version("next", service)
    }
}
