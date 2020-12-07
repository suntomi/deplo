use std::error::Error;
use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::config;
use crate::endpoints;
use crate::plan;
use crate::module;

#[derive(Serialize, Deserialize, Debug)]
pub struct CreateBucketOption {
    pub region: Option<String>
}
#[derive(Serialize, Deserialize, Debug)]
pub struct DeployStorageOption {
    pub permission: Option<String>,
    pub max_age: Option<u32>,
    pub excludes: Option<String>, //valid on directory copy
    pub destination: String,
    pub region: Option<String>
}
pub enum StorageKind<'a> {
    Service {
        plan: &'a plan::Plan
    },
    Metadata {
        lb_name: &'a str,
        version: u32
    }
}
pub trait Cloud : module::Module {
    fn new(
        config: &config::Container, account_name: &str
    ) -> Result<Self, Box<dyn Error>> where Self : Sized;
    // terraformer
    fn generate_terraformer_config(&self, name: &str) -> Result<String, Box<dyn Error>>;
    // dns
    fn root_domain_dns_name(&self) -> Result<String, Box<dyn Error>>;
    // container 
    fn push_container_image(
        &self, src: &str, target: &str, options: &HashMap<String, String>
    ) -> Result<String, Box<dyn Error>>;
    fn deploy_container(
        &self, plan: &plan::Plan,
        target: &plan::ContainerDeployTarget,
        // note: ports always contain single entry corresponding to the empty string key
        image: &str, services: &HashMap<String, plan::Port>, 
        env: &HashMap<String, String>,
        options: &HashMap<String, String>
    ) -> Result<(), Box<dyn Error>>;
    // storage
    fn create_bucket(
        &self, bucket_name: &str, options: &CreateBucketOption
    ) -> Result<(), Box<dyn Error>>;
    fn delete_bucket(
        &self, bucket_name: &str
    ) -> Result<(), Box<dyn Error>>;
    fn deploy_storage<'b>(
        &self, kind: StorageKind<'b>, 
        //copymap]: src => dest option. if src ends with /, directory copy
        copymap: &HashMap<String, DeployStorageOption>
    ) -> Result<(), Box<dyn Error>>;
    // load balancer
    fn update_path_matcher(
        &self, lb_name: &str, endpoints: &endpoints::Endpoints
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
fn factory_by<'a, T: Cloud + 'a>(
    config: &config::Container,
    account_name: &str
) -> Result<Box<dyn Cloud + 'a>, Box<dyn Error>> {
    let cmd = T::new(config, account_name)?;
    return Ok(Box::new(cmd) as Box<dyn Cloud + 'a>);
}

pub fn factory<'a>(
    config: &config::Container,
    account_name: &str
) -> Result<Box<dyn Cloud + 'a>, Box<dyn Error>> {
    match &config.borrow().cloud.account(account_name) {
        config::CloudProviderConfig::GCP {..} => {
            return factory_by::<gcp::Gcp>(config, account_name);
        },
        _ => return Err(Box::new(CloudError {
            cause: format!("add factory matching pattern for account[{}]", account_name)
        }))
    };
}
