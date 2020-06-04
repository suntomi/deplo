use std::collections::HashMap;
use std::error::Error;
use std::path;
use std::fs;
use std::fmt;

use maplit::hashmap;
use serde::{Deserialize, Serialize};

use crate::config;
use crate::plan;
use crate::util::escalate;

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

#[derive(Serialize, Deserialize, PartialEq, Clone)]
pub enum ChangeType {
    None,
    Path,
    Version,
}
impl fmt::Display for ChangeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Path => write!(f, "path"),
            Self::Version => write!(f, "version"),
        }
    }
}


#[derive(Serialize, Deserialize, Clone)]
pub struct Release {
    pub paths: Option<HashMap<String, String>>,
    pub endpoint_service_map: HashMap<String, String>,
    pub versions: HashMap<String, u32>,
}
impl Release {
    pub fn get_version(&self, service: &str) -> u32 {
        match self.versions.get(service) {
            Some(v) => *v,
            None => 0
        }
    }
}

impl PartialEq for Release {
    fn eq(&self, other: &Self) -> bool {
        self.versions == other.versions
    }
}

#[derive(Serialize, Deserialize)]
pub struct Endpoints {
    pub version: u32,
    pub host: String,
    pub confirm_deploy: Option<bool>,
    pub latest_front_only: Option<bool>,
    pub default: Option<String>,
    pub paths: Option<HashMap<String, String>>,
    pub min_front_versions: HashMap<String, u32>,
    pub next: Release,
    pub releases: Vec<Release>,
    pub deploy_state: Option<DeployState>,
}

impl Endpoints {
    pub fn new(host: &str) -> Endpoints {
        Endpoints {
            version: 0,
            host: host.to_string(),
            confirm_deploy: None,
            latest_front_only: None,
            default: None,
            paths: None,
            min_front_versions: hashmap!{},
            next: Release {
                paths: None,
                endpoint_service_map: hashmap!{},
                versions: hashmap!{},
            },
            releases: vec!(),
            deploy_state: None,
        }
    }
    pub fn load<P: AsRef<path::Path>>(config: &config::Config, path: P) -> Result<Endpoints, Box<dyn Error>> {
        match fs::read_to_string(path) {
            Ok(content) => {
                let ep = serde_json::from_str::<Endpoints>(&content)?;
                ep.verify(config)?;
                Ok(ep)
            },
            Err(err) => escalate!(Box::new(err))
        }        
    }
    pub fn save<P: AsRef<path::Path>>(&self, path: P) -> Result<(), Box<dyn Error>> {
        let as_text = serde_json::to_string_pretty(&self)?;
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
    pub fn path_will_change(&self, config: &config::Config) -> Result<ChangeType, Box<dyn Error>> {
        let mut change = ChangeType::None;
        let vs = &self.next.versions;
        for (ep, _) in vs {
            if self.version_changed(ep) {
                if change != ChangeType::Path {
                    change = ChangeType::Version;
                }
                let service = match config.find_service_by_endpoint(ep) {
                    Some(s) => s,
                    None => continue
                };
                let plan = plan::Plan::load(config, service)?;
                if plan.has_deployment_of("service")? {
                    change = ChangeType::Path;
                }
            }
        }
        return Ok(change)
    }
    pub fn cascade_versions(
        &mut self, config: &config::Config
    ) -> Result<(), Box<dyn Error>> {
        self.next.endpoint_service_map = config.runtime.endpoint_service_map.clone();
        self.releases.insert(0, self.next.clone());
        self.persist(config)?;

        Ok(())
    }
    pub fn version_up(&mut self, config: &config::Config) -> Result<(), Box<dyn Error>> {
        self.version += 1;
        self.persist(config)?;
        Ok(())
    }
    pub fn get_latest_version(&self, service: &str) -> u32 {
        if self.releases.len() > 0 {
            self.releases[0].get_version(service)
        } else {
            0
        }
    }

    fn verify(&self, _: &config::Config) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn target<'a>(&'a self) -> &'a str {
        self.host.split(".").collect::<Vec<&str>>()[0]
    }
    fn version_changed(&self, service: &str) -> bool {
        self.get_latest_version(service) != self.next.get_version(service)
    }
}
