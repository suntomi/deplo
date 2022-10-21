use std::collections::HashMap;
use std::error::Error;

use crate::config;
use crate::module;
use crate::shell;

mod runner;

pub trait Step {
    fn new(
        config: &config::Container,
        module_key: String
    ) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn run(
        &self, shell_settings: &shell::Settings,
        envs: &HashMap<String, config::Value>,
        with: &Option<HashMap<String, config::AnyValue>>
    ) -> Result<String, Box<dyn Error>>;
}

#[derive(Clone)]
pub struct ModuleDescription;
impl module::Description for ModuleDescription {
    fn ty() -> config::module::Type { return config::module::Type::Step; }
}

fn factory_by<'a, T: Step + 'a>(
    config: &config::Container,
    module_key: String
) -> Result<Box<dyn Step + 'a>, Box<dyn Error>> {
    let cmd = T::new(config, module_key)?;
    return Ok(Box::new(cmd) as Box<dyn Step + 'a>);
}

pub fn factory<'a>(
    config: &config::Container,
    module_key: String
) -> Result<Box<dyn Step + 'a>, Box<dyn Error>> {
    factory_by::<runner::ModuleRunner>(config, module_key)
}
