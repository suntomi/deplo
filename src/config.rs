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
use indexmap::IndexMap;
use glob::glob;

use crate::args;
use crate::vcs;
use crate::cloud;
use crate::lb;
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
// now unified DNS zone for multiple cloud provider, is not supported. 
// you must have dedicated DNS zone for each provider which required to setup load balancer.
// I assume you can use CNAME to unify domain.
pub enum CloudProviderConfig {
    GCP {
        key: String,
        project_id: String,
        dns_zone: String,
        region: String
    },
    AWS {
        key_id: String,
        secret_key: String,
        dns_zone: String,
        region: String
    },
    ALI {
        key_id: String,
        secret_key: String,
        region: String
    },
    AZR {
        subscription_id: String,
        tenant_id: String,
        region: String
    }
}
impl CloudProviderConfig {
    fn code(&self) -> String {
        match self {
            Self::GCP{key:_,project_id:_,dns_zone:_,region:_} => "GCP".to_string(),
            Self::AWS{key_id:_, secret_key:_,dns_zone:_,region:_} => "AWS".to_string(),
            Self::ALI{key_id:_, secret_key:_,region:_} => "ALI".to_string(),
            Self::AZR{subscription_id:_,tenant_id:_,region:_} => "AZR".to_string(),
        }
    }
}
impl fmt::Display for CloudProviderConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GCP{key:_,project_id:_,dns_zone:_,region:_} => write!(f, "gcp"),
            Self::AWS{key_id:_, secret_key:_,dns_zone:_,region:_} => write!(f, "aws"),
            Self::ALI{key_id:_, secret_key:_,region:_} => write!(f, "ali"),
            Self::AZR{subscription_id:_,tenant_id:_,region:_} => write!(f, "azr"),
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
        backend: Option<String>,
        backend_bucket: String,
        resource_prefix: Option<String>,
    }
}
impl fmt::Display for TerraformerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Terraform { 
                backend: _,
                backend_bucket: _,
                resource_prefix: _
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
    pub fn resource_prefix<'b>(&'b self) -> &'b Option<String> {
        match &self.terraformer {
            TerraformerConfig::Terraform { 
                backend: _,
                backend_bucket: _,
                resource_prefix
            } => resource_prefix
        }
    }
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
    pub project_namespace: String,
    pub deplo_image: String,
    pub data_dir: String,
    pub no_confirm_for_prod_deploy: bool,
    pub release_targets: HashMap<String, String>,
}
#[derive(Default)]
pub struct RuntimeConfig {
    pub verbosity: u64,
    pub dryrun: bool,
    pub debug: HashMap<String, String>,
    pub distributions: Vec<String>,
    pub latest_endpoint_versions: HashMap<String, u32>,
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
        // fill missing default configuration
        {
            let mut mutc = c.ptr.borrow_mut();
            match mutc.lb.get("default") {
                Some(_) => {},
                None => {
                    // default load balancer which uses default account (and its provider)
                    mutc.lb.insert("default".to_string(), LoadBalancerConfig{ account: None });
                }
            }
        }
        // setup runtime configuration (except release_target)
        {
            let mut mutc = c.ptr.borrow_mut();
            mutc.runtime = RuntimeConfig {
                verbosity,
                distributions: vec!(),
                latest_endpoint_versions: hashmap!{},
                endpoint_service_map: hashmap!{},
                dryrun: args.occurence_of("dryrun") > 0,
                debug: match args.values_of("debug") {
                    Some(s) => {
                        let mut opts = hashmap!{};
                        for v in s {
                            let sp: Vec<&str> = v.split("=").collect();
                            opts.insert(
                                sp[0].to_string(),
                                (if sp.len() > 1 { sp[1] } else { "true" }).to_string()
                            );
                        }
                        opts
                    }
                    None => hashmap!{}
                },
                release_target: None, // set after
                workdir: may_workdir.map(String::from),
            };
        }
        // verify all configuration files, including endoints/plans
        let (latest_endpoint_versions, distributions, endpoint_service_map) = Self::verify(&c)?;
        {
            let mut mutc = c.ptr.borrow_mut();
            mutc.runtime.latest_endpoint_versions = latest_endpoint_versions;
            mutc.runtime.distributions = distributions;
            mutc.runtime.endpoint_service_map = endpoint_service_map;
        }
        {
            log::debug!("====== latest endpoint versions ====== ");
            let c = c.ptr.borrow();
            for (ep, v) in &c.runtime.latest_endpoint_versions {
                log::debug!("{} => {}", ep, v);
            }
        }
        // setup module cache
        Self::ensure_ci_init(&c)?;
        Self::ensure_cloud_init(&c)?;
        Self::ensure_vcs_init(&c)?;
        Self::ensure_tf_init(&c)?;
        // set release target
        {
            // because vcs_service create object which have reference of `c` ,
            // scope of `vcs` should be narrower than this function,
            // to prevent `assignment of borrowed value` error below.
            let release_target = {
                let immc = c.ptr.borrow();
                let vcs = immc.vcs_service()?;
                vcs.release_target()
            };
            let mut mutc = c.ptr.borrow_mut();
            mutc.runtime.release_target = match release_target {
                Some(v) => Some(v),
                None => mutc.get_debug_option("force_set_release_target_to").map_or(
                    None, |v| Some(v.clone())
                )
            }
        }
        // do preparation
        let reinit = args.value_of("reinit").unwrap_or("none");
        Self::prepare_ci(&c, reinit == "all" || reinit == "ci")?;
        Self::prepare_cloud(&c, reinit == "all" || reinit == "cloud")?;
        Self::prepare_vcs(&c, reinit == "all" || reinit == "vcs")?;
        Self::prepare_tf(&c, reinit == "all" || reinit == "tf")?;

        // do lb preparation
        lb::prepare(&c)?;
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
    pub fn endpoints_file_path(&self, release_target: Option<&str>) -> path::PathBuf {
        let p = self.endpoints_path();
        if let Some(e) = release_target {
            return p.join(format!("{}.json", e));
        } else if let Some(e) = self.release_target() {
            return p.join(format!("{}.json", e));
        } else {
            panic!("should be on release target branch")
        }
    }
    pub fn project_namespace(&self) -> &str {
        &self.common.project_namespace
    }
    pub fn root_domain(&self) -> Result<String, Box<dyn Error>> {
        let cloud = self.cloud_service("default")?;
        let dns_name = cloud.root_domain_dns_name()?;
        Ok(format!("{}.{}", self.project_namespace(), dns_name[..dns_name.len()-1].to_string()))
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
        format!("{}-{}-{}", self.project_namespace(), 
            self.release_target().expect("should be on release target branch"),
            prefixed_name
        )
    }
    pub fn latest_endpoint_version(&self, endpoint: &str) -> u32 {
        match self.runtime.latest_endpoint_versions.get(endpoint) {
            Some(v) => *v,
            None => {
                let service = self.runtime.endpoint_service_map.get(endpoint).expect(
                    &format!("endpoint:{} should exist in plan file", endpoint)
                );
                if service == endpoint { 0 } else { 
                    self.latest_endpoint_version(service)
                }
            }
        }
    }
    pub fn version_changed(&self, endpoint: &str, release: &endpoints::Release) -> bool {
        self.latest_endpoint_version(endpoint) != release.get_version(endpoint)
    }
    pub fn next_endpoint_version(&self, endpoint: &str) -> u32 {
        self.latest_endpoint_version(endpoint) + 1
    }
    pub fn update_endpoint_version(
        c: &Container, lb_name: &str, endpoint: &str, plan: &plan::Plan
    ) -> Result<u32, Box<dyn Error>> {
        endpoints::Endpoints::modify(c, c.borrow().endpoints_file_path(None), |eps| {
            // latest version of endpoint is service version which the endpoint belongs to
            let next_version = c.borrow().next_endpoint_version(endpoint);
            let next = eps.prepare_next_if_not_exist(c);
            let deployments = next.versions.entry(lb_name.to_string()).or_insert(IndexMap::new());
            let vs = deployments.entry(plan.deployment_kind()?).or_insert(IndexMap::new());
            vs.insert(endpoint.to_string(), next_version);
            // if confirm_deploy is not set and this is deployment of distribution, 
            // automatically update min_front_version with new version
            if eps.certify_latest_dist_only.unwrap_or(false) && 
                plan.has_deployment_of(plan::DeployKind::Distribution)? &&
                endpoint == &plan.service {
                let fv = eps.min_certified_dist_versions
                    .entry(endpoint.to_string()).or_insert(0);
                // set next version
                *fv = next_version;
            }
            return Ok(next_version);
        })
    }
    pub fn default_backend(&self) -> String {
        format!("{}-backend-bucket-404", self.project_namespace())
    }
    pub fn lb_config<'a>(&'a self, lb_name: &str) -> &'a LoadBalancerConfig {
        match self.lb.get(lb_name) {
            Some(c) => &c,
            None => panic!("no lb config for {}", lb_name)
        }
    }
    pub fn lb_names_for_provider<'b>(&'b self, provider_name: &str) -> Vec<&'b str> {
        let mut list = vec!();
        for (name, lb_config) in &self.lb {
            let default = &"default".to_string();
            let lb_account = lb_config.account.as_ref().unwrap_or(default);
            let account = match self.cloud.accounts.get(lb_account) {
                Some(a) => a,
                None => panic!("account {} does not exist", lb_account)
            };
            if account.code().to_lowercase() == provider_name.to_lowercase() {
                list.push(&**name)
            }
        }
        list
    }
    pub fn cloud_service<'a>(&'a self, account_name: &str) -> Result<&'a Box<dyn cloud::Cloud>, Box<dyn Error>> {
        return match self.cloud_caches.get(account_name) {
            Some(cloud) => Ok(cloud),
            None => escalate!(Box::new(ConfigError{ 
                cause: format!("no cloud service for {}", account_name) 
            }))
        }
    }
    pub fn cloud_provider_and_configs<'a>(&'a self) -> HashMap<String, &'a CloudProviderConfig> {
        let mut h = HashMap::<String, &'a CloudProviderConfig>::new();
        for (_, provider_config) in &self.cloud.accounts {
            let code = provider_config.code();
            match h.get_mut(&code) {
                Some(_) => { 
                    panic!("currently multiple account for same provider {} is not supported", code)
                },
                None => { h.insert(code, provider_config); }
            }
        }
        return h
    }
    pub fn account_name_from_provider_config<'a>(&'a self, config: &CloudProviderConfig) -> Option<&'a str> {
        for (name, provider_config) in &self.cloud.accounts {
            let code = provider_config.code();
            if code == config.code() {
                return Some(name)
            }
        }
        return None
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
            Err(e) =>  {
                match self.get_debug_option("ci_env") {
                    Some(v) => {
                        for (account_name, config) in &self.ci {
                            if config.type_matched(&v) { return (account_name, config) }
                        };
                    },
                    None => {}
                }
                panic!("DEPLO_CI_TYPE is not defined, should be development mode {}", e)
            }
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
    fn prepare_ci(c: &Container, reinit: bool) -> Result<(), Box<dyn Error>> {
        let c = c.ptr.borrow();
        let cache = &c.ci_caches;
        for (_, ci) in cache {
            ci.prepare(reinit)?;
        }
        Ok(())
    }
    fn ensure_ci_init(c: &Container) -> Result<(), Box<dyn Error>> {
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
    fn prepare_cloud(c: &Container, reinit: bool) -> Result<(), Box<dyn Error>> {
        let c = c.ptr.borrow();
        let cache = &c.cloud_caches;
        for (_, ci) in cache {
            ci.prepare(reinit)?;
        }
        Ok(())

    }
    fn ensure_cloud_init(c: &Container) -> Result<(), Box<dyn Error>> {
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
    fn prepare_vcs(c: &Container, reinit: bool) -> Result<(), Box<dyn Error>> {
        let c = c.ptr.borrow();
        let cache = &c.vcs_cache;
        if cache.len() <= 0 {
            return escalate!(Box::new(ConfigError{ 
                cause: format!("no vcs service") 
            }))
        }
        cache[0].prepare(reinit)
    }
    fn ensure_vcs_init(c: &Container) -> Result<(), Box<dyn Error>> {
        let vcs = vcs::factory(c)?;
        let mut mutc = c.ptr.borrow_mut();
        mutc.vcs_cache.push(vcs);
        Ok(())
    }
    fn prepare_tf(c: &Container, reinit: bool) -> Result<(), Box<dyn Error>> {
        let c = c.ptr.borrow();
        let cache = &c.tf_cache;
        if cache.len() <= 0 {
            return escalate!(Box::new(ConfigError{ 
                cause: format!("no vcs service") 
            }))
        }
        cache[0].prepare(reinit)
    }    
    fn ensure_tf_init(c: &Container) -> Result<(), Box<dyn Error>> {
        let tf = tf::factory(c)?;
        let mut mutc = c.ptr.borrow_mut();
        mutc.tf_cache.push(tf);
        Ok(())
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
        self.runtime.debug.get(name) != None
    }
    pub fn get_debug_option<'a>(&'a self, name: &str) -> Option<&'a String> {
        self.runtime.debug.get(name)
    }

    fn verify(
        c: &Container
    ) -> Result<
        (HashMap<String, u32>,Vec<String>,HashMap<String, String>),
        Box<dyn Error>
    > {
        log::debug!("verify config");
        // global configuration verificaitons
        // 1. all endpoints/plans can be loaded without error 
        //    (loading endpoints/plans verify consistency of its content)
        // 2. keys in each plan's extra_endpoints and plan name is project-unique
        //    => this means no endpoint belongs to multiple load balancer too
        // 3. ports belongs to same service must belong to load balancers of same cloud provider account
        //    => this restriction plans to be removed by deploying same service to multiple cloud provider account
        let mut latest_endpoint_versions_and_path: HashMap<String, (String, u32, usize)> = hashmap!{};
        let mut endpoint_service_map: HashMap<String, String> = hashmap!{};
        let mut distributions = vec!();
        for entry in glob(&c.borrow().endpoints_path().join("*.json").to_string_lossy())? {
            match entry {
                Ok(path) => {
                    let endpoints = endpoints::Endpoints::load(&c, &path)?;
                    if endpoints.releases.len() > 0 {
                        for (idx, r) in endpoints.releases.iter().enumerate() {
                            for (_, deployments) in &r.versions {
                                for (_, vs) in deployments {
                                    for (ep, v) in vs {
                                        match latest_endpoint_versions_and_path.get(ep) {
                                            Some(ent) => if ent.2 == idx {
                                                return Err(Box::new(ConfigError {
                                                    cause: format!(
                                                        "endpoint name:{} duplicates in endpoint {} and {} @ rhis:{}",
                                                        ep, path.to_string_lossy(), ent.0, idx
                                                    )
                                                }))
                                            },
                                            None => {
                                                latest_endpoint_versions_and_path.insert(
                                                    ep.to_string(),
                                                    (path.to_string_lossy().to_string(), *v, idx)
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                Err(e) => return Err(Box::new(e))
            }
        }
        for entry in glob(&c.borrow().services_path().join("*.toml").to_string_lossy())? {
            match entry {
                Ok(path) => {
                    log::debug!("verify config: load at path {}", path.to_string_lossy());
                    let distribution_name = {
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
                                let mut lb_cloud_account_name: Option<String> = None;
                                for (n, port) in &ports {
                                    let name = if n.is_empty() { &plan.service } else { n };
                                    match endpoint_service_map.get(name) {
                                        Some(service_name) => return Err(Box::new(ConfigError {
                                            cause: format!(
                                                "endpoint name:{} both exists in plan {}.toml and {}.toml",
                                                name, service_name, plan.service
                                            )
                                        })),
                                        None => {
                                            let account_name = port.get_lb_cloud_account_name(&c, &plan);
                                            if match &lb_cloud_account_name {
                                                Some(name) => *name == account_name,
                                                None => {
                                                    lb_cloud_account_name = Some(account_name.clone());
                                                    true
                                                }
                                            } {
                                                endpoint_service_map.insert(name.to_string(), plan.service.clone());
                                            } else {
                                                return Err(Box::new(ConfigError {
                                                    cause: format!(
                                                        "endpoint name:{} uses cloud acount {} to deploy \
                                                        but other endpoint uses another cloud account {}",
                                                        name, account_name, lb_cloud_account_name.unwrap()
                                                    )
                                                }))
                                            }
                                        }
                                    }
                                }
                            },
                            None => {
                                match endpoint_service_map.get(&plan.service) {
                                    Some(service_name) => return Err(Box::new(ConfigError {
                                        cause: format!(
                                            "endpoint name:{} both exists in plan {}.toml and {}.toml",
                                            plan.service, service_name, plan.service
                                        )
                                    })),
                                    None => {
                                        endpoint_service_map.insert(
                                            plan.service.clone(), plan.service.clone()
                                        );
                                    }
                                }
                            }
                        }
                        if plan.has_deployment_of(plan::DeployKind::Distribution)? {
                            plan.service.clone()
                        } else {
                            "".to_string()
                        }
                    };
                    if !distribution_name.is_empty() {
                        distributions.push(distribution_name);
                    }
                },
                Err(e) => return Err(Box::new(e))
            }
        }
        Ok(({
            let mut latest_endpoint_versions: HashMap<String, u32> = hashmap!{};
            for (k, v) in latest_endpoint_versions_and_path {
                latest_endpoint_versions.insert(k, v.1);
            };
            latest_endpoint_versions
        }, distributions, endpoint_service_map))
    }
}
