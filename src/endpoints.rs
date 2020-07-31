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
    ConfirmCleanup,
    AfterCleanup,
}
impl fmt::Display for DeployState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid => write!(f, "invalid"),
            Self::ConfirmCleanup => write!(f, "confirm_cleanup"),
            Self::AfterCleanup => write!(f, "after_cleanup"),
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


#[derive(Serialize, Deserialize, Clone, Debug)]
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
    pub fn has_service(&self, service: &str, version: u32) -> bool {
        return self.get_version(service) == version
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
    pub lb_name: String,
    pub host: String,
    pub confirm_deploy: Option<bool>,
    pub certify_latest_dist_only: Option<bool>,
    pub backport_target_branch: Option<String>,
    pub default: Option<String>,
    pub paths: Option<HashMap<String, String>>,
    pub min_certified_dist_versions: HashMap<String, u32>,
    pub next: Release,
    pub releases: Vec<Release>,
    pub deploy_state: Option<DeployState>,
}

impl Endpoints {
    pub fn new(lb_name: &str, host: &str) -> Endpoints {
        Endpoints {
            version: 0,
            lb_name: lb_name.to_string(),
            host: host.to_string(),
            confirm_deploy: None,
            certify_latest_dist_only: None,
            backport_target_branch: None,
            default: None,
            paths: None,
            min_certified_dist_versions: hashmap!{},
            next: Release {
                paths: None,
                endpoint_service_map: hashmap!{},
                versions: hashmap!{},
            },
            releases: vec!(),
            deploy_state: None,
        }
    }
    pub fn load<P: AsRef<path::Path>>(config: &config::Container, path: P) -> Result<Endpoints, Box<dyn Error>> {
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
    fn persist(&self, config: &config::Ref) -> Result<(), Box<dyn Error>> {
        self.save(config.endpoints_file_path(&self.lb_name, Some(self.target())))
    }
    pub fn modify<P: AsRef<path::Path>, F, R: Sized>(
        config: &config::Container, path: P, f: F
    ) -> Result<R, Box<dyn Error>> 
    where F: Fn(&mut Endpoints) -> Result<R, Box<dyn Error>> {
        let mut ep = Self::load(config, &path)?;
        let r = f(&mut ep)?;
        ep.save(&path)?;
        Ok(r)
    }
    pub fn cloud_account_name(&self, config: &config::Ref) -> String {
        config.lb_config(&self.lb_name).account_name().to_string()
    }
    pub fn change_type<'a>(&self, config: &config::Container) -> Result<ChangeType, Box<dyn Error>> {
        let config_ref = config.borrow();
        let mut change = ChangeType::None;
        let vs = &self.next.versions;
        for (ep, _) in vs {
            if self.version_changed(ep) {
                if change != ChangeType::Path {
                    change = ChangeType::Version;
                }
                let service = match config_ref.find_service_by_endpoint(ep) {
                    Some(s) => s,
                    None => continue
                };
                let plan = plan::Plan::load(&config, service)?;
                if plan.has_deployment_of("service")? {
                    change = ChangeType::Path;
                }
            }
        }
        return Ok(change)
    }
    pub fn cascade_releases(
        &mut self, config: &config::Ref
    ) -> Result<(), Box<dyn Error>> {
        self.next.endpoint_service_map = config.runtime.endpoint_service_map.clone();
        self.releases.insert(0, self.next.clone());
        self.persist(config)?;

        Ok(())
    }
    pub fn version_up(&mut self, config: &config::Ref) -> Result<(), Box<dyn Error>> {
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
    pub fn service_is_active(&self, service: &str, version: u32) -> bool {
        if self.next.has_service(service, version) {
            return true;
        }
        for r in &self.releases {
            if r.has_service(service, version) {
                return true;
            }
        }
        return false;
    }
    pub fn set_deploy_state(
        &mut self, config: &config::Ref, deploy_state: Option<DeployState>
    ) -> Result<(), Box<dyn Error>> {
        self.deploy_state = deploy_state;
        self.persist(config)?;
        Ok(())
    }
    pub fn gc_releases(
        &mut self, config: &config::Ref
    ) -> Result<bool, Box<dyn Error>> {
        let mut marked_releases: Vec<Release> = vec!();
        for r in &self.releases {
            let mut referred = true;
            for (service, min_version) in &self.min_certified_dist_versions {
                if r.get_version(service) < *min_version {
                    referred = false;
                    break;
                }
            }
            if referred {
                // if version is same as last pushed release
                // for all services in min_certified_dist_versions,
                // that release will not marked, because higher version tuple
                // can handle these versions of front services
                if marked_releases.len() > 0 {
                    let last_pushed = &marked_releases[marked_releases.len() - 1];
                    let mut front_versions_same = true;
                    for (service, _) in &self.min_certified_dist_versions {
                        if last_pushed.get_version(service) != r.get_version(service) {
                            front_versions_same = false;
                        }
                    }
                    if front_versions_same {
                        continue;
                    }
                }
                marked_releases.push(r.clone());
            } else {
                log::debug!("release {:?} collected", r);
            }
        }
        let collected = marked_releases.len() != self.releases.len();
        self.releases = marked_releases;
        self.persist(config)?;
        return Ok(collected);
    }

    fn verify(&self, _: &config::Container) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn target<'a>(&'a self) -> &'a str {
        self.host.split(".").collect::<Vec<&str>>()[0]
    }
    fn version_changed(&self, service: &str) -> bool {
        self.get_latest_version(service) != self.next.get_version(service)
    }
}
