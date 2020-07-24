use std::error::Error;
use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::config;
use crate::endpoints;
use crate::plan;

#[derive(Serialize, Deserialize, Debug)]
pub struct DeployStorageOption {
    pub permission: Option<String>,
    pub max_age: Option<u32>,
    pub excludes: Option<String>, //valid on directory copy
    pub destination: String
}
pub enum StorageKind<'a> {
    Service {
        plan: &'a plan::Plan<'a>
    },
    Metadata {
        lb_name: &'a str,
        version: u32
    }
}
pub trait Cloud<'a> {
    fn new(
        config: &'a config::Config, account_name: &str
    ) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn setup_dependency(&self) -> Result<(), Box<dyn Error>>;
    fn cleanup_dependency(&self) -> Result<(), Box<dyn Error>>;
    fn generate_terraformer_config(&self, name: &str) -> Result<String, Box<dyn Error>>;
    // dns
    fn root_domain_dns_name(&self, zone: &str) -> Result<String, Box<dyn Error>>;
    // container 
    fn push_container_image(&self, src: &str, target: &str) -> Result<String, Box<dyn Error>>;
    fn deploy_container(
        &self, plan: &plan::Plan<'a>,
        target: &plan::ContainerDeployTarget,
        // note: ports always contain single entry corresponding to the empty string key
        image: &str, ports: &HashMap<String, u32>, 
        env: &HashMap<String, String>,
        options: &HashMap<String, String>
    ) -> Result<(), Box<dyn Error>>;
    // storage
    fn create_bucket(
        &self, bucket_name: &str
    ) -> Result<(), Box<dyn Error>>;
    fn deploy_storage<'b>(
        &self, kind: StorageKind<'b>, 
        //copymap]: src => dest option. if src ends with /, directory copy
        copymap: &HashMap<String, DeployStorageOption>
    ) -> Result<(), Box<dyn Error>>;
    // load balancer
    fn update_path_matcher(
        &self, endpoints: &endpoints::Endpoints
    ) -> Result<(), Box<dyn Error>>;
}

#[derive(Debug)]
pub struct CloudError {
    cause: String
}
impl fmt::Display for CloudError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl Error for CloudError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

// providers
pub mod gcp;


// factorys
fn factory_by<'a, T: Cloud<'a> + 'a>(
    config: &'a config::Config,
    account_name: &str
) -> Result<Box<dyn Cloud<'a> + 'a>, Box<dyn Error>> {
    let cmd = T::new(config, account_name).unwrap();
    cmd.setup_dependency()?;
    return Ok(Box::new(cmd) as Box<dyn Cloud<'a> + 'a>);
}

pub fn factory<'a>(
    config: &'a config::Config,
    account_name: &str
) -> Result<Box<dyn Cloud<'a> + 'a>, Box<dyn Error>> {
    match &config.cloud.account(account_name) {
        config::CloudProviderConfig::GCP {key:_} => {
            return factory_by::<gcp::Gcp>(config, account_name);
        },
        _ => return Err(Box::new(CloudError {
            cause: format!("add factory matching pattern for account[{}]", account_name)
        }))
    };
}
