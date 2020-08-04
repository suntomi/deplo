use std::fs;
use std::fmt;
use std::path;
use std::error::Error;
use std::rc::Rc;
use std::cell::{RefCell};
use std::collections::{HashMap};

use log;
use simple_logger;
use serde::{Deserialize, Serialize};
use dotenv::dotenv;
use maplit::hashmap;
use glob::glob;

use crate::args;
use crate::vcs;
use crate::cloud;
use crate::tf;
use crate::ci;
use crate::endpoints;
use crate::plan;
use crate::util::{escalate,envsubst};

pub const DEPLO_GIT_HASH: &'static str = env!("GIT_HASH");

#[derive(Debug)]
pub struct ConfigError {
    pub cause: String
}
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CloudProviderConfig {
    GCP {
        key: String
    },
    AWS {
        key_id: String,
        secret_key: String
    },
    ALI {
        key_id: String,
        secret_key: String
    }
}
impl CloudProviderConfig {
    fn code(&self) -> String {
        match self {
            Self::GCP{key:_} => "GCP".to_string(),
            Self::AWS{key_id:_, secret_key:_} => "AWS".to_string(),
            Self::ALI{key_id:_, secret_key:_} => "ALI".to_string(),
        }
    }
}
impl fmt::Display for CloudProviderConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GCP{key:_} => write!(f, "gcp"),
            Self::AWS{key_id:_, secret_key:_} => write!(f, "aws"),
            Self::ALI{key_id:_, secret_key:_} => write!(f, "ali"),
        }
    }    
}
#[derive(Serialize, Deserialize)]
pub struct ActionConfig {
    pub pr: HashMap<String, String>,
    pub deploy: HashMap<String, String>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CIConfig {
    GhAction {
        account: String,
        key: String,
        action: ActionConfig
    },
    Circle {
        key: String,
        action: ActionConfig
    }
}
impl CIConfig {
    pub fn type_matched(&self, t: &str) -> bool {
        match self {
            Self::GhAction{key:_,account:_, action:_} => t == "GhAction",
            Self::Circle{key:_, action:_} => t == "Circle"
        }
    }
    pub fn action<'a>(&'a self) -> &'a ActionConfig {
        match &self {
            Self::GhAction{key:_,account:_, action} => action,
            Self::Circle{key:_, action} => action
        }
    }
}
impl fmt::Display for CIConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GhAction{key:_,account:_, action:_} => write!(f, "github-action"),
            Self::Circle{key:_, action:_} => write!(f, "circle"),
        }
    }    
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum VCSConfig {
    Github {
        email: String,
        account: String,
        key: String
    },
    Gitlab {
        email: String,
        account: String,
        key: String
    }
}
impl fmt::Display for VCSConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Github{ email:_, account:_, key:_ } => write!(f, "github"),
            Self::Gitlab{ email:_, account:_, key:_ } => write!(f, "gitlab"),
        }
    }    
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TerraformerConfig {
    Terraform {
        backend_bucket: String,
        resource_prefix: Option<String>,
        dns_zone: String,
        region: String,
    }
}
impl TerraformerConfig {
    pub fn dns_zone(&self) -> &str {
        match self {
            Self::Terraform { 
                backend_bucket: _,
                resource_prefix: _,
                dns_zone,
                region: _
            } => &dns_zone
        }
    }
    pub fn region(&self) -> &str {
        match self {
            Self::Terraform { 
                backend_bucket: _,
                resource_prefix: _,
                dns_zone: _,
                region
            } => &region
        }
    }
}
impl fmt::Display for TerraformerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Terraform { 
                backend_bucket: _,
                resource_prefix: _,
                dns_zone: _,
                region: _
            } => write!(f, "terraform")
        }
    }    
}
#[derive(Serialize, Deserialize)]
pub struct CloudConfig {
    pub accounts: HashMap<String, CloudProviderConfig>,
    pub terraformer: TerraformerConfig,
}
impl CloudConfig {
    pub fn account<'b>(&'b self, name: &str) -> &'b CloudProviderConfig {
        match &self.accounts.get(name) {
            Some(a) => a,
            None => panic!("provider corresponding to account {} does not exist", name)
        }
    }
    fn infra_code_dest_root_path(&self, config: &Config) -> path::PathBuf {
        path::Path::new(&config.common.data_dir).join("infra")
    }
    fn infra_code_dest_path(&self, config: &Config, provider_code: &str) -> path::PathBuf {
        self.infra_code_dest_root_path(config).join(provider_code)
    }
    fn infra_code_path(&self, config: &Config, provider_code: &str) -> path::PathBuf {
        config.resource_root_path().join("infra").join(provider_code)
    }
}
#[derive(Serialize, Deserialize)]
pub struct LoadBalancerConfig {
    pub account: Option<String>
}
impl LoadBalancerConfig {
    pub fn account_name<'a>(&'a self) -> &'a str {
        match &self.account {
            Some(a) => a,
            None => "default"
        }
    }
}
#[derive(Serialize, Deserialize)]
pub struct CommonConfig {
    pub project_id: String,
    pub deplo_image: String,
    pub data_dir: String,
    pub no_confirm_for_prod_deploy: bool,
    pub release_targets: HashMap<String, String>,
}
#[derive(Default)]
pub struct RuntimeConfig {
    pub verbosity: u64,
    pub dryrun: bool,
    pub debug: Vec<String>,
    pub store_deployments: Vec<String>,
    pub endpoint_service_map: HashMap<String, String>,
    pub release_target: Option<String>,
    pub workdir: Option<String>,
}
#[derive(Serialize, Deserialize)]
pub struct Config {
    #[serde(skip)]
    pub runtime: RuntimeConfig,
    pub common: CommonConfig,
    pub cloud: CloudConfig,
    pub vcs: VCSConfig,
    pub lb: HashMap<String, LoadBalancerConfig>,
    pub ci: HashMap<String, CIConfig>,

    // object cache
    #[serde(skip)]
    pub ci_caches: HashMap<String, Box<dyn ci::CI>>,
    #[serde(skip)]
    pub cloud_caches: HashMap<String, Box<dyn cloud::Cloud>>,
    // HACK: I don't know the way to have uninitialized box object...
    #[serde(skip)]
    vcs_cache: Vec<Box<dyn vcs::VCS>>,
    #[serde(skip)]
    tf_cache: Vec<Box<dyn tf::Terraformer>>
}
pub type Ref<'a>= std::cell::Ref<'a, Config>;
#[derive(Clone)]
pub struct Container {
    ptr: Rc<RefCell<Config>>
}
impl Container { 
    pub fn borrow(&self) -> Ref<'_> {
        self.ptr.borrow()
    }
}

impl Config {
    // static factory methods 
    pub fn load(path: &str) -> Result<Config, Box<dyn Error>> {
        let src = fs::read_to_string(path).unwrap();
        let content = envsubst(&src);
        match toml::from_str(&content) {
            Ok(c) => Ok(c),
            Err(err) => escalate!(Box::new(err))
        }
    }
    pub fn create<A: args::Args>(args: &A) -> Result<Container, Box<dyn Error>> {
        // apply working directory
        let may_workdir = args.value_of("workdir");
        match may_workdir {
            Some(wd) => { std::env::set_current_dir(&wd)?; },
            None => {}
        }
        // apply verbosity
        let verbosity = args.occurence_of("verbosity");
        simple_logger::init_with_level(match verbosity {
            0 => log::Level::Warn,
            1 => log::Level::Info,
            2 => log::Level::Debug,
            3 => log::Level::Trace,
            _ => log::Level::Warn
        }).unwrap();
        // load dotenv
        match args.value_of("dotenv") {
            Some(dotenv_path) => match fs::metadata(dotenv_path) {
                Ok(_) => { dotenv::from_filename(dotenv_path).unwrap(); },
                Err(_) => { dotenv::from_readable(dotenv_path.as_bytes()).unwrap(); },
            },
            None => match dotenv() {
                Ok(path) => log::debug!("using .env file at {}", path.to_string_lossy()),
                Err(err) => match std::env::var("DEPLO_CI_TYPE") {
                    Ok(_) => {},
                    Err(_) => log::warn!(".env not present or cannot load by error [{:?}], this usually means:\n\
                        1. command will be run with incorrect parameter or\n\
                        2. secrets are directly written in deplo.toml\n\
                        please use .env to store secrets, or use -e flag to specify its path", 
                        err)
                } 
            },
        };
        // println!("DEPLO_CLOUD_ACCESS_KEY:{}", std::env::var("DEPLO_CLOUD_ACCESS_KEY").unwrap());    
        let c = Container{
            ptr: Rc::new(RefCell::new(
                Config::load(args.value_of("config").unwrap_or("./deplo.toml")).unwrap()
            ))
        };
        // setup runtime configuration (except release_target)
        {
            let mut mutc = c.ptr.borrow_mut();
            mutc.runtime = RuntimeConfig {
                verbosity,
                store_deployments: vec!(),
                endpoint_service_map: hashmap!{},
                dryrun: args.occurence_of("dryrun") > 0,
                debug: match args.values_of("debug") {
                    Some(s) => s.iter().map(|e| e.to_string()).collect(),
                    None => vec!{}
                },
                release_target: None, // set after
                workdir: may_workdir.map(String::from),
            };
        }
        // verify all configuration files, including endoints/plans
        let (endpoint_service_map, store_deployments) = Self::verify(&c)?;
        {
            let mut mutc = c.ptr.borrow_mut();
            mutc.runtime.endpoint_service_map = endpoint_service_map;
            mutc.runtime.store_deployments = store_deployments;
        }
        // setup module cache
        Self::ensure_ci_init(&c)?;
        Self::ensure_cloud_init(&c)?;
        Self::ensure_vcs_init(&c)?;
        Self::ensure_tf_init(&c)?;
        // set release target
        {
            let mut mutc = c.ptr.borrow_mut();
            // because vcs_service create object which have reference of `c` ,
            // scope of `vcs` should be narrower than this function,
            // to prevent `assignment of borrowed value` error below.
            let vcs = mutc.vcs_service()?;
            mutc.runtime.release_target = vcs.release_target();
        }
        return Ok(c);
    }
    pub fn root_path(&self) -> &path::Path {
        return path::Path::new(&self.common.data_dir);
    }
    pub fn resource_root_path(&self) -> &path::Path {
        return path::Path::new("/rsc");
    }
    pub fn services_path(&self) -> path::PathBuf {
        return path::Path::new(&self.common.data_dir).join("services");
    }
    pub fn endpoints_path(&self) -> path::PathBuf {
        return path::Path::new(&self.common.data_dir).join("endpoints");
    }
    pub fn endpoints_file_path(&self, lb_name: &str, release_target: Option<&str>) -> path::PathBuf {
        let mut p = self.endpoints_path();
        if lb_name != "default" {
            p = p.join(lb_name)
        }
        if let Some(e) = release_target {
            return p.join(format!("{}.json", e));
        } else if let Some(e) = self.release_target() {
            return p.join(format!("{}.json", e));
        } else {
            panic!("should be on release target branch")
        }
    }
    pub fn project_id(&self) -> &str {
        &self.common.project_id
    }
    pub fn root_domain(&self) -> Result<String, Box<dyn Error>> {
        let cloud = self.cloud_service("default")?;
        let dns_name = cloud.root_domain_dns_name(&self.cloud.terraformer.dns_zone())?;
        Ok(format!("{}.{}", self.common.project_id, dns_name[..dns_name.len()-1].to_string()))
    }
    pub fn release_target(&self) -> Option<&str> {
        match &self.runtime.release_target {
            Some(s) => Some(&s),
            None => None
        }
    }
    pub fn infra_code_source_path(&self, provider_code: &str) -> path::PathBuf {
        self.cloud.infra_code_path(&self, provider_code)
    }
    pub fn infra_code_dest_path(&self, provider_code: &str) -> path::PathBuf {
        self.cloud.infra_code_dest_path(&self, provider_code)
    }
    pub fn infra_code_dest_root_path(&self) -> path::PathBuf {
        self.cloud.infra_code_dest_root_path(&self)
    }
    pub fn canonical_name(&self, prefixed_name: &str) -> String {
        format!("{}-{}-{}", self.project_id(), 
            self.release_target().expect("should be on release target branch"),
            prefixed_name
        )
    }
    pub fn endpoint_version(c: &Container, lb_name: &str, endpoint: &str) -> Result<u32, Box<dyn Error>> {
        match endpoints::Endpoints::load(c, c.borrow().endpoints_file_path(lb_name, None)) {
            Ok(eps) => Ok(eps.get_latest_version(endpoint)),
            Err(err) => escalate!(err)
        }
    }
    pub fn next_endpoint_version(c: &Container, lb_name: &str, endpoint: &str) -> Result<u32, Box<dyn Error>> {
        Ok(Self::endpoint_version(c, lb_name, endpoint)? + 1)
    }
    pub fn update_endpoint_version(
        c: &Container, lb_name: &str, endpoint: &str, plan: &plan::Plan
    ) -> Result<u32, Box<dyn Error>> {
        endpoints::Endpoints::modify(c, c.borrow().endpoints_file_path(lb_name, None), |eps| {
            let latest_version = eps.get_latest_version(endpoint);
            let v = eps.next.versions.entry(endpoint.to_string()).or_insert(0);
            *v = latest_version + 1;
            // if confirm_deploy is not set and this is deployment of distribution, 
            // automatically update min_front_version with new version
            if eps.certify_latest_dist_only.unwrap_or(false) && 
                plan.has_deployment_of("distribution")? &&
                endpoint == &plan.service {
                let fv = eps.min_certified_dist_versions
                    .entry(endpoint.to_string()).or_insert(0);
                // set next version
                *fv = *v;
            }
            return Ok(*v);
        })
    }
    pub fn default_backend(&self) -> String {
        format!("{}-backend-bucket-404", self.common.project_id)
    }
    pub fn lb_config<'a>(&'a self, lb_name: &str) -> &'a LoadBalancerConfig {
        match self.lb.get(lb_name) {
            Some(c) => &c,
            None => panic!("no lb config for {}", lb_name)
        }
    }
    pub fn cloud_service<'a>(&'a self, account_name: &str) -> Result<&'a Box<dyn cloud::Cloud>, Box<dyn Error>> {
        return match self.cloud_caches.get(account_name) {
            Some(cloud) => Ok(cloud),
            None => escalate!(Box::new(ConfigError{ 
                cause: format!("no cloud service for {}", account_name) 
            }))
        }
    }
    pub fn cloud_provider_and_configs<'a>(&'a self) -> HashMap<String, Vec<&'a CloudProviderConfig>> {
        let mut h = HashMap::<String, Vec<&'a CloudProviderConfig>>::new();
        for (_, provider_config) in &self.cloud.accounts {
            let code = provider_config.code();
            match h.get_mut(&code) {
                Some(v) => { v.push(provider_config); },
                None => { h.insert(code, vec!(provider_config)); }
            }
        }
        return h
    }
    pub fn terraformer<'a>(&'a self) -> Result<&'a Box<dyn tf::Terraformer>, Box<dyn Error>> {
        return if self.tf_cache.len() > 0 {
            Ok(&self.tf_cache[0])
        } else {
            escalate!(Box::new(ConfigError{ 
                cause: format!("no tf service") 
            }))            
        }
    }
    pub fn ci_config<'a>(&'a self, account_name: &str) -> &'a CIConfig {
        match &self.ci.get(account_name) {
            Some(c) => c,
            None => panic!("provider corresponding to account {} does not exist", account_name)
        }
    }
    pub fn ci_config_by_env<'b>(&'b self) -> (&'b str, &'b CIConfig) {
        match std::env::var("DEPLO_CI_TYPE") {
            Ok(v) => { 
                for (account_name, config) in &self.ci {
                    if config.type_matched(&v) { return (account_name, config) }
                }
                panic!("DEPLO_CI_TYPE = {}, but does not have corresponding CI Config", v)
            },
            // TODO: returns merged action
            Err(e) => panic!("DEPLO_CI_TYPE is not defined, should be development mode {}", e)
        }
    }
    pub fn ci_service<'a>(&'a self, account_name: &str) -> Result<&'a Box<dyn ci::CI>, Box<dyn Error>> {
        return match self.ci_caches.get(account_name) {
            Some(ci) => Ok(ci),
            None => escalate!(Box::new(ConfigError{ 
                cause: format!("no ci service for {}", account_name) 
            }))
        } 
    }
    pub fn ensure_ci_init(c: &Container) -> Result<(), Box<dyn Error>> {
        let mut caches = hashmap!{};
        {
            let immc = c.borrow();
            // default always should exist
            let _ = &immc.ci.get("default").unwrap();
            for (account, _) in &immc.ci {
                caches.insert(account.to_string(), ci::factory(c, account)?);
            }
        }
        {
            let mut mutc = c.ptr.borrow_mut();
            mutc.ci_caches = caches;
        }
        Ok(())
    }
    pub fn ensure_cloud_init(c: &Container) -> Result<(), Box<dyn Error>> {
        let mut caches = hashmap!{};
        {
            let immc = c.borrow();
            // default always should exist
            let _ = &immc.cloud.accounts.get("default").unwrap();
            for (account, _) in &immc.cloud.accounts {
                caches.insert(account.to_string(), cloud::factory(c, account)?);
            }
        }
        {
            let mut mutc = c.ptr.borrow_mut();
            mutc.cloud_caches = caches;
        }
        Ok(())
    }
    pub fn ensure_vcs_init(c: &Container) -> Result<(), Box<dyn Error>> {
        let mut mutc = c.ptr.borrow_mut();
        let vcs = vcs::factory(c)?;
        mutc.vcs_cache.push(vcs);
        Ok(())
    }
    pub fn ensure_tf_init(c: &Container) -> Result<(), Box<dyn Error>> {
        let mut mutc = c.ptr.borrow_mut();
        let tf = tf::factory(c)?;
        mutc.tf_cache.push(tf);
        Ok(())
    }    
    pub fn cloud_region<'a>(&'a self) -> &'a str {
        return self.cloud.terraformer.region();
    }
    pub fn cloud_resource_name(&self, path: &str) -> Result<String, Box<dyn Error>> {
        return self.terraformer()?.eval(path);
    }
    pub fn vcs_service<'a>(&'a self) -> Result<&'a Box<dyn vcs::VCS>, Box<dyn Error>> {
        return if self.vcs_cache.len() > 0 {
            Ok(&self.vcs_cache[0])
        } else {
            escalate!(Box::new(ConfigError{ 
                cause: format!("no vcs service") 
            }))            
        }
    }
    pub fn has_debug_option(&self, name: &str) -> bool {
        match self.runtime.debug.iter().position(|e| e == name) {
            Some(_) => true,
            None => false
        }
    }
    pub fn find_service_by_endpoint(&self, endpoint: &str) -> Option<&String> {
        self.runtime.endpoint_service_map.get(endpoint)
    }    

    fn verify(c: &Container) -> Result<(HashMap<String,String>,Vec<String>), Box<dyn Error>> {
        log::debug!("verify config");
        // global configuration verificaiton
        // 1. all endpoints/plans can be loaded without error 
        //    (loading endpoints/plans verify consistency of its content)
        // 2. keys in each plan's extra_ports is project-unique
        let mut endpoint_service_map = hashmap!{};
        let mut store_deployments = vec!();
        for entry in glob(&c.borrow().services_path().join("*.toml").to_string_lossy())? {
            match entry {
                Ok(path) => {
                    log::debug!("verify config: load at path {}", path.to_string_lossy());
                    let store_deployment_name = {
                        let plan = match plan::Plan::load_by_path(&c, &path) {
                            Ok(p) => p,
                            Err(err) => return escalate!(Box::new(ConfigError {
                                cause: format!(
                                    "verify config error loading {} by {:?}", 
                                    path.to_string_lossy(), err
                                )
                            }))
                        };
                        match plan.ports()? {
                            Some(ports) => {
                                for (n, _) in &ports {
                                    let name = if n.is_empty() { &plan.service } else { n };
                                    match endpoint_service_map.get(name) {
                                        Some(service_name) => return Err(Box::new(ConfigError {
                                            cause: format!(
                                                "endpoint name:{} both exists in plan {}.toml and {}.toml",
                                                name, service_name, plan.service
                                            )
                                        })),
                                        None => {
                                            endpoint_service_map.entry(name.to_string()).or_insert(plan.service.clone());
                                        }
                                    }
                                }
                            },
                            None => {
                                endpoint_service_map.entry(plan.service.clone()).or_insert(plan.service.clone());
                            }
                        }
                        if plan.has_deployment_of("distribution")? {
                            plan.service.clone()
                        } else {
                            "".to_string()
                        }
                    };
                    if !store_deployment_name.is_empty() {
                        store_deployments.push(store_deployment_name);
                    }
                },
                Err(e) => return Err(Box::new(e))
            }
        }
        Ok((endpoint_service_map, store_deployments))
    }
}
