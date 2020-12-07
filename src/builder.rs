use std::error::Error;
use std::fmt;

use crate::config;
use crate::plan;
use crate::module;

pub trait Builder : module::Module {
    fn new(
        config: &config::Container, builder_config: &plan::BuilderConfig
    ) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn build(
        &self,
        org_name: String,
        app_name: String,
        app_id: String,
        project_path: String,
        artifact_path: Option<String>
    ) -> Result<(), Box<dyn Error>>;
}

#[derive(Debug)]
pub struct BuilderError {
    cause: String
}
impl fmt::Display for BuilderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl Error for BuilderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

// subcommands
pub mod unity;

// factorys
fn factory_by<'a, T: Builder + 'a>(
    config: &config::Container,
    builder_config: &plan::BuilderConfig
) -> Result<Box<dyn Builder + 'a>, Box<dyn Error>> {
    let cmd = T::new(config, builder_config)?;
    return Ok(Box::new(cmd) as Box<dyn Builder + 'a>);
}

pub fn factory<'a>(
    config: &config::Container,
    builder_config: &plan::BuilderConfig
) -> Result<Box<dyn Builder + 'a>, Box<dyn Error>> {
    match builder_config {
        plan::BuilderConfig::Unity {
            unity_version: _,
            serial_code: _,
            account: _,
            password: _,
            platform: _
        } => {
            return factory_by::<unity::Unity>(config, builder_config);
        },
        _ => return Err(Box::new(BuilderError {
            cause: format!("add factory matching pattern for [{:?}]", builder_config)
        }))
    };
}
