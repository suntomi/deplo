use std::error::Error;
use std::path;
use std::fs;
use std::fmt;

use serde::{Deserialize, Serialize};
use indexmap::IndexMap;

use crate::config;
use crate::plan;
use crate::util::escalate;

#[derive(Debug)]
pub struct EndpointError {
    pub cause: String
}
impl fmt::Display for EndpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl Error for EndpointError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

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
    pub paths: Option<IndexMap<String, String>>,
    // { distribution name => its version }
    pub distributions: IndexMap<String, u32>,
    // lb_name => { deploy_kind => { endpoint_name => its version } }
    pub versions: IndexMap<String, plan::Deployments>,
}
impl Release {
    pub fn get_version(&self, service: &str) -> u32 {
        for (_, lbm) in &self.versions {
            for (_, eps) in lbm {
                match eps.get(service) {
                    Some(v) => return *v,
                    None => continue
                }        
            }
        }
        return 0
    }
    pub fn get_dist_version(&self, service: &str) -> u32 {
        *self.distributions.get(service).unwrap_or(&0)
    }
    pub fn has_endpoint(&self, endpoint: &str, version: u32) -> bool {
        return self.get_version(endpoint) == version
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
    pub target: String,
    pub host_postfix: String,
    pub confirm_deploy: Option<bool>,
    pub certify_latest_dist_only: Option<bool>,
    pub backport_target_branch: Option<String>,
    pub default: Option<IndexMap<String, Option<String>>>,
    pub paths: Option<IndexMap<String, String>>,
    pub min_certified_dist_versions: IndexMap<String, u32>,
    pub next: Option<Release>, // None if no plan to release
    pub releases: Vec<Release>,
    pub deploy_state: Option<DeployState>,
}

impl Endpoints {
    pub fn new(target: &str, host_postfix: &str) -> Endpoints {
        Endpoints {
            version: 0,
            target: target.to_string(),
            host_postfix: host_postfix.to_string(),
            confirm_deploy: None,
            certify_latest_dist_only: None,
            backport_target_branch: None,
            default: None,
            paths: None,
            min_certified_dist_versions: IndexMap::new(),
            next: None,
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
    pub fn dump(&self) -> Result<String, Box<dyn Error>> {
        Ok(serde_json::to_string_pretty(&self)?)
    }
    fn persist(&self, config: &config::Ref) -> Result<(), Box<dyn Error>> {
        self.save(config.endpoints_file_path(Some(self.target())))
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
    pub fn target_host(
        &self, lb_name: &str
    ) -> String {
        if lb_name == "default" {
            format!("{}.{}", self.target, self.host_postfix)
        } else {
            format!("{}.{}.{}", self.target, lb_name, self.host_postfix)
        }
    }
    pub fn change_type(
        &self, lb_name: &str, config: &config::Container
    ) -> Result<ChangeType, Box<dyn Error>> {
        let config_ref = config.borrow();
        let mut change = ChangeType::None;
        let next = self.get_next().unwrap_or(&self.releases[0]);
        let vs = match next.versions.get(lb_name) {
            Some(entry) => entry,
            None => if self.releases.len() > 0 {
                match self.releases[0].versions.get(lb_name) {
                    // if previous version has load balancer entry, path should change
                    Some(_) => return Ok(ChangeType::Path),
                    // both prev/next have no entry for the load balancer, thus no change
                    None => return Ok(change)
                }
            } else {
                // first release, but does not contains the load balancer, means no change
                return Ok(change)
            }
        };
        for (kind, eps) in vs {
            for (ep, _) in eps {
                if config_ref.version_changed(ep, next) {
                    if change != ChangeType::Path {
                        change = ChangeType::Version;
                    }
                    if *kind == plan::DeployKind::Service {
                        change = ChangeType::Path;
                    }
                }
            }
        }
        return Ok(change)
    }
    pub fn prepare_next_if_not_exist<'a>(
        &'a mut self, config: &config::Container
    ) -> &'a mut Release {
        if self.next.is_none() {
            if self.releases.len() > 0 {
                self.next = Some(self.releases[0].clone());
            } else {
                self.next = Some(Release {
                    paths: None,
                    distributions: IndexMap::new(),
                    versions: IndexMap::new()
                });
            }
        }
        let next = self.next.as_mut().unwrap();
        for (lb_name, deployments) in &mut next.versions {
            for (_, versions) in deployments {
                versions.retain(|ep,_| {
                    let plan = match plan::Plan::find_by_endpoint(config, &ep) {
                        Ok(p) => p,
                        Err(e) => {
                            log::debug!("lb:{}, ep {}, cannot find plan {:?}", lb_name, ep, e);
                            return false
                        }
                    };
                    match plan.ports() {
                        Ok(ports) => match ports {
                            Some(ps) => match ps.get(ep) {
                                Some(port) => {
                                    log::debug!("lb:{}, ep:{}, port {}/{}", lb_name, ep,
                                        port.port, port.lb_name.as_ref().unwrap_or(&"default".to_string()));
                                    port.get_lb_name(&plan) == *lb_name
                                },
                                None => {
                                    log::debug!("lb:{}, ep:{}, no port entry service:{}", lb_name, ep, plan.service);
                                    ep.to_string() == plan.service
                                }
                            },
                            None => {
                                log::debug!("lb:{}, ep:{}, no container", lb_name, ep);
                                true // ep is storage or distribution
                            }
                        }
                        Err(e) => {
                            log::debug!("lb:{}, ep:{}, get ports error {:?}", lb_name, ep, e);
                            false
                        }
                    }
                });
            }
        }
        return next;
    }
    pub fn cascade_releases(
        &mut self, config: &config::Container
    ) -> Result<(), Box<dyn Error>> {
        self.releases.insert(0, self.get_next()?.clone());
        self.next = None;
        self.persist(&config.borrow())?;
        Ok(())
    }
    pub fn version_up(&mut self, config: &config::Ref) -> Result<(), Box<dyn Error>> {
        self.version += 1;
        self.persist(config)?;
        Ok(())
    }
    pub fn service_is_active(&self, service: &str, version: u32) -> Result<bool, Box<dyn Error>> {
        match self.get_next() {
            Ok(r) => if r.has_endpoint(service, version) {
                return Ok(true);
            },
            Err(_) => {}
        }
        for r in &self.releases {
            if r.has_endpoint(service, version) {
                return Ok(true);
            }
        }
        return Ok(false);
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
                if r.get_dist_version(service) < *min_version {
                    log::debug!("release {:?} will be collected: service {} version {} <  min certified {}", 
                        r, service, r.get_dist_version(service), *min_version);
                    referred = false;
                    break;
                }
            }
            if referred {
                // if version is same as last pushed release
                // for all services in min_certified_dist_versions,
                // that release will not marked, because newer version tuple
                // can handle these version of distributions
                if marked_releases.len() > 0 {
                    let last_pushed = &marked_releases[marked_releases.len() - 1];
                    let mut front_versions_same = true;
                    for (service, _) in &self.min_certified_dist_versions {
                        if last_pushed.get_dist_version(service) != r.get_dist_version(service) {
                            front_versions_same = false;
                        }
                    }
                    if front_versions_same {
                        log::debug!("release {:?} will be collected: distributions are same {:?} as newer release {:?}",
                            r, r.distributions, last_pushed.distributions);
                        continue;
                    }
                }
                marked_releases.push(r.clone());
            } else {
                log::debug!("release {:?} collected", r);
            }
        }
        let collected = marked_releases.len() != self.releases.len();
        if collected {
            log::debug!("some releases are collected {} => {}",
                self.releases.len(), marked_releases.len()
            );
        } else {
            log::debug!("no releases are collected {}", self.releases.len());
        }
        self.releases = marked_releases;
        self.persist(config)?;
        return Ok(collected);
    }

    fn verify(&self, _: &config::Container) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn target<'a>(&'a self) -> &'a str {
        &self.target
    }
    fn get_next<'a>(&'a self) -> Result<&'a Release, Box<dyn Error>> {
        match &self.next {
            Some(n) => Ok(n),
            None => escalate!(Box::new(EndpointError {
                cause: "no next release".to_string()
            }))
        }
    } 
}
