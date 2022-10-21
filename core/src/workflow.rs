use std::error::Error;

use crate::config;
use crate::module;
use crate::shell;

mod runner;

pub trait Workflow {
    fn new(
        config: &config::Container,
        module_key: String
    ) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn listen(&self) -> Result<(), Box<dyn Error>>;
    fn matches(&self, event: &str, params: &str) -> Result<bool, Box<dyn Error>>;
}
#[derive(Clone)]
pub struct ModuleDescription;
impl module::Description for ModuleDescription {
    fn ty() -> config::module::Type { return config::module::Type::Workflow; }
}

fn factory_by<'a, T: Workflow + 'a>(
    config: &config::Container,
    module_key: String
) -> Result<Box<dyn Workflow + 'a>, Box<dyn Error>> {
    let cmd = T::new(config, module_key)?;
    return Ok(Box::new(cmd) as Box<dyn Workflow + 'a>);
}

pub fn factory<'a>(
    config: &config::Container,
    module_key: String
) -> Result<Box<dyn Workflow + 'a>, Box<dyn Error>> {
    factory_by::<runner::ModuleRunner>(config, module_key)
}
