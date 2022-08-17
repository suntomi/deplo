use std::collections::HashMap;
use std::error::Error;

use crate::config;
use crate::module;

pub trait Workflow {
    fn new(config: &config::Container, params: &HashMap<String, config::AnyValue>) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn listen(&self) -> Result<(), Box<dyn Error>>;
    fn matches(&self, event: &str, params: &str) -> Result<bool, Box<dyn Error>>;
}
#[derive(Clone)]
pub struct Module;
impl module::Module for Module {
    fn ty() -> config::module::Type { return config::module::Type::Workflow; }
}

pub struct Dummy;
impl Workflow for Dummy {
    fn new(_: &config::Container, _: &HashMap<String, config::AnyValue>) -> Result<Self, Box<dyn Error>> {
        return Ok(Dummy);
    }
    fn listen(&self) -> Result<(), Box<dyn Error>> {
        return Ok(());
    }
    fn matches(&self, _: &str, _: &str) -> Result<bool, Box<dyn Error>> {
        return Ok(false);
    }
}

pub fn factory<'a>(
    config: &config::Container,
    uses: &config::Value,
    with: &Option<HashMap<String, config::AnyValue>>
) -> Result<Box<dyn Workflow + 'a>, Box<dyn Error>> {
    log::error!("workflow plugin is not implemented yet");
    return Ok(Box::new(Dummy));
}
