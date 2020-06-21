use std::fs;
use std::fmt;
use std::path;
use std::error::Error;
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
    fn infra_code_path(&self, config: &Config) -> path::PathBuf {
        let base = config.resource_root_path().join("infra");
        match self {
            Self::GCP{key:_} => base.join("gcp"),
            Self::AWS{key_id:_, secret_key:_} => base.join("aws"),
            Self::ALI{key_id:_, secret_key:_} => base.join("ali"),
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
#[serde(tag = "type")]
pub enum CIConfig {
    GhAction {
        account: String,
        key: String
    },
    Circle {
        key: String
    }
}
impl CIConfig {
    pub fn type_matched(&self, t: &str) -> bool {
        match self {
            Self::GhAction{key:_,account:_} => t == "GhAction",
            Self::Circle{key:_} => t == "Circle"
        }
    }
}
impl fmt::Display for CIConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GhAction{key:_,account:_} => write!(f, "github-action"),
            Self::Circle{key:_} => write!(f, "circle"),
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
    pub provider: CloudProviderConfig,
    pub terraformer: TerraformerConfig,
}
#[derive(Serialize, Deserialize)]
pub struct CommonConfig {
    pub project_id: String,
    pub deplo_image: String,
    pub data_dir: String,
    pub no_confirm_for_prod_deploy: bool,
    pub release_targets: HashMap<String, String>,
}
#[derive(Serialize, Deserialize)]
pub struct ActionConfig {
    pub pr: HashMap<String, String>,
    pub deploy: HashMap<String, String>,
}
#[derive(Default)]
pub struct RuntimeConfig<'a> {
    pub verbosity: u64,
    pub dryrun: bool,
    pub debug: Vec<&'a str>,
    pub store_deployments: Vec<String>,
    pub endpoint_service_map: HashMap<String, String>,
    pub release_target: Option<String>,
    pub workdir: Option<String>,
}
#[derive(Serialize, Deserialize)]
pub struct Config<'a> {
    #[serde(skip)]
    pub runtime: RuntimeConfig<'a>,
    pub common: CommonConfig,
    pub cloud: CloudConfig,
    pub vcs: VCSConfig,
    pub ci: CIConfig,
    pub action: ActionConfig
}

impl<'a> Config<'a> {
    // static factory methods 
    pub fn load(path: &str) -> Result<Config, Box<dyn Error>> {
        let src = fs::read_to_string(path).unwrap();
        let content = envsubst(&src);
        match toml::from_str(&content) {
            Ok(c) => Ok(c),
            Err(err) => escalate!(Box::new(err))
        }
    }
    pub fn create<A: args::Args>(args: &A) -> Result<Config, Box<dyn Error>> {
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
        let mut c = Config::load(args.value_of("config").unwrap_or("./deplo.toml")).unwrap();
        c.runtime = RuntimeConfig {
            verbosity,
            store_deployments: vec!(),
            endpoint_service_map: hashmap!{},
            dryrun: args.occurence_of("dryrun") > 0,
            debug: match args.values_of("debug") {
                Some(s) => s,
                None => vec!{}
            },
            release_target: {
                // because vcs_service create object which have reference of `c` ,
                // scope of `vcs` should be narrower than this function,
                // to prevent `assignment of borrowed value` error below.
                let vcs = c.vcs_service()?;
                vcs.release_target()
            },
            workdir: may_workdir.map(String::from),
        };
        // verify all configuration files, including endoints/plans
        c.verify()?;
        // create service objects and invoke associated setup
        let _ = c.ci_service()?;
        let _ = c.cloud_service()?;
        
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
    pub fn project_id(&self) -> &str {
        return &self.common.project_id
    }
    pub fn root_domain(&self) -> Result<String, Box<dyn Error>> {
        let cloud = self.cloud_service()?;
        let dns_name = cloud.root_domain_dns_name(&self.cloud.terraformer.dns_zone())?;
        Ok(format!("{}.{}", self.common.project_id, dns_name[..dns_name.len()-1].to_string()))
    }
    pub fn release_target(&self) -> Option<&str> {
        return match &self.runtime.release_target {
            Some(s) => Some(&s),
            None => None
        }
    }
    pub fn infra_code_source_path(&self) -> path::PathBuf {
        return self.cloud.provider.infra_code_path(&self);
    }
    pub fn infra_code_dest_path(&self) -> path::PathBuf {
        return path::Path::new(&self.common.data_dir).join("infra");
    }
    pub fn canonical_name(&self, prefixed_name: &str) -> String {
        return format!("{}-{}-{}", self.project_id(), 
            self.release_target().expect("should be on release target branch"),
            prefixed_name
        )
    }
    pub fn service_endpoint_version(&'a self, endpoint: &str) -> Result<u32, Box<dyn Error>> {
        match endpoints::Endpoints::load(&self, &self.endpoints_file_path(None)) {
            Ok(eps) => Ok(eps.get_latest_version(endpoint)),
            Err(err) => escalate!(err)
        }
    }
    pub fn next_service_endpoint_version(&self, endpoint: &str) -> Result<u32, Box<dyn Error>> {
        Ok(self.service_endpoint_version(endpoint)? + 1)
    }
    pub fn update_service_endpoint_version(
        &self, endpoint: &str, plan: &plan::Plan
    ) -> Result<u32, Box<dyn Error>> {
        endpoints::Endpoints::modify(&self, &self.endpoints_file_path(None), |eps| {
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
    pub fn cloud_service(&'a self) -> Result<Box<dyn cloud::Cloud<'a> + 'a>, Box<dyn Error>> {
        return cloud::factory(&self);
    }
    pub fn terraformer(&'a self) -> Result<Box<dyn tf::Terraformer<'a> + 'a>, Box<dyn Error>> {
        return tf::factory(&self);
    }
    pub fn ci_service(&'a self) -> Result<Box<dyn ci::CI<'a> + 'a>, Box<dyn Error>> {
        return ci::factory(&self);
    }
    pub fn cloud_region(&'a self) -> &str {
        return self.cloud.terraformer.region();
    }
    pub fn cloud_resource_name(&self, path: &str) -> Result<String, Box<dyn Error>> {
        return self.terraformer()?.eval(path);
    }
    pub fn vcs_service(&'a self) -> Result<Box<dyn vcs::VCS<'a> + 'a>, Box<dyn Error>> {
        return vcs::factory(&self);
    }
    pub fn has_debug_option(&self, name: &str) -> bool {
        match self.runtime.debug.iter().position(|&e| e == name) {
            Some(_) => true,
            None => false
        }
    }
    pub fn find_service_by_endpoint(&self, endpoint: &str) -> Option<&String> {
        self.runtime.endpoint_service_map.get(endpoint)
    }
    pub fn has_action_config(&self) -> bool {
        self.action.pr.len() + self.action.deploy.len() > 0
    }
    
    fn verify(&mut self) -> Result<(), Box<dyn Error>> {
        log::debug!("verify config");
        // global configuration verificaiton
        // 1. all endpoints/plans can be loaded without error 
        //    (loading endpoints/plans verify consistency of its content)
        // 2. keys in each plan's extra_ports is project-unique
        let mut services = hashmap!{};
        for entry in glob(&self.services_path().join("*.toml").to_string_lossy())? {
            match entry {
                Ok(path) => {
                    log::debug!("verify config: load at path {}", path.to_string_lossy());
                    let store_deployment_name = {
                        let plan = match plan::Plan::load_by_path(&self, &path) {
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
                                    match services.get(name) {
                                        Some(service_name) => return Err(Box::new(ConfigError {
                                            cause: format!(
                                                "endpoint name:{} both exists in plan {}.toml and {}.toml",
                                                name, service_name, plan.service
                                            )
                                        })),
                                        None => {
                                            services.entry(name.to_string()).or_insert(plan.service.clone());
                                        }
                                    }
                                }
                            },
                            None => {
                                services.entry(plan.service.clone()).or_insert(plan.service.clone());
                            }
                        }
                        if plan.has_deployment_of("distribution")? {
                            plan.service.clone()
                        } else {
                            "".to_string()
                        }
                    };
                    if !store_deployment_name.is_empty() {
                        self.runtime.store_deployments.push(store_deployment_name);
                    }
                },
                Err(e) => return Err(Box::new(e))
            }
        }
        self.runtime.endpoint_service_map = services;
        Ok(())
    }
}
