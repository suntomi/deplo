use std::error::Error;
use std::fmt;

use crate::config;
use crate::cloud;
use crate::module;

pub trait Terraformer : module::Module {
    fn new(config: &config::Container) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn destroy(&self);
    fn init(&self) -> Result<(), Box<dyn Error>>;
    fn plan(&self) -> Result<(), Box<dyn Error>>;
    fn apply(&self) -> Result<(), Box<dyn Error>>;
    fn rm(&self, path: &str) -> Result<(), Box<dyn Error>>;
    fn rclist(&self) -> Result<Vec<String>, Box<dyn Error>>;
    fn eval(&self, path: &str) -> Result<String, Box<dyn Error>>;
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
fn factory_by<'a, T: Terraformer + 'a>(
    config: &config::Container
) -> Result<Box<dyn Terraformer + 'a>, Box<dyn Error>> {
    let cmd = T::new(config).unwrap();
    return Ok(Box::new(cmd) as Box<dyn Terraformer + 'a>);
}

pub fn factory<'a>(
    config: &config::Container
) -> Result<Box<dyn Terraformer + 'a>, Box<dyn Error>> {
    match &config.borrow().cloud.terraformer {
        config::TerraformerConfig::Terraform {
            backend:_,
            backend_bucket: _,
            resource_prefix: _
        } => {
            return factory_by::<terraform::Terraform>(config);
        }
    };
}
