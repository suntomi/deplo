use std::error::Error;
use std::collections::HashMap;
use std::fmt;

use crate::config;
use crate::command::service::plan;

pub trait Cloud<'a> {
    fn new(config: &'a config::Config) -> Result<Self, Box<dyn Error>> where Self : Sized;
    // container 
    fn push_container_image(&self, src: &str, target: &str) -> Result<String, Box<dyn Error>>;
    fn deploy_container(
        &self, plan: &plan::Plan,
        target: &plan::DeployTarget,
        image: &str, ports: &Vec<u32>, 
        env: &HashMap<String, String>,
        options: &HashMap<String, String>
    ) -> Result<(), Box<dyn Error>>;
    // storage
    fn create_bucket(
        &self, bucket_name: &str
    ) -> Result<(), Box<dyn Error>>;
    fn deploy_storage(
        &self, copymap: &HashMap<String, String>
    ) -> Result<(), Box<dyn Error>>;
    // load balancer
    fn deploy_load_balancer(
        &self
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
    config: &'a config::Config
) -> Result<Box<dyn Cloud<'a> + 'a>, Box<dyn Error>> {
    let cmd = T::new(config).unwrap();
    return Ok(Box::new(cmd) as Box<dyn Cloud<'a> + 'a>);
}

pub fn factory<'a>(
    config: &'a config::Config
) -> Result<Box<dyn Cloud<'a> + 'a>, Box<dyn Error>> {
    match &config.cloud.provider {
        config::CloudProviderConfig::GCP { key: _ } => {
            return factory_by::<gcp::Gcp>(config);
        },
        _ => return Err(Box::new(CloudError {
            cause: format!("add factory matching pattern for [{}]", config.cloud.provider)
        }))
    };
}
