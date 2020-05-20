use std::error::Error;
use std::fmt;

use crate::config;
use crate::cloud;

pub trait Terraformer<'a> {
    fn new(config: &'a config::Config) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn init(&self, cloud: &Box<dyn cloud::Cloud<'a> + 'a>) -> Result<(), Box<dyn Error>>;
    fn plan(&self) -> Result<(), Box<dyn Error>>;
    fn apply(&self) -> Result<(), Box<dyn Error>>;
    fn exec(&self) -> Result<(), Box<dyn Error>> {
        self.plan()?;
        self.apply()
    }
}

#[derive(Debug)]
pub struct TerraformerError {
    cause: String
}
impl fmt::Display for TerraformerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl Error for TerraformerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

// providers
pub mod terraform;


// factorys
fn factory_by<'a, T: Terraformer<'a> + 'a>(
    config: &'a config::Config
) -> Result<Box<dyn Terraformer<'a> + 'a>, Box<dyn Error>> {
    let cmd = T::new(config).unwrap();
    return Ok(Box::new(cmd) as Box<dyn Terraformer<'a> + 'a>);
}

pub fn factory<'a>(
    config: &'a config::Config
) -> Result<Box<dyn Terraformer<'a> + 'a>, Box<dyn Error>> {
    match &config.cloud.terraformer {
        config::TerraformerConfig::Terraform {
            backend_bucket: _,
            backend_bucket_prefix: _,
            root_domain: _,
            region: _
        } => {
            return factory_by::<terraform::Terraform>(config);
        },
        _ => return Err(Box::new(TerraformerError {
            cause: format!("add factory matching pattern for [{}]", config.cloud.terraformer)
        }))
    };
}
