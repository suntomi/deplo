use std::collections::HashMap;
use std::error::Error;

use crate::config;
use crate::module;

pub trait Workflow : module::Module {
    fn new(config: &config::Container, params: &HashMap<String, config::AnyValue>) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn listen(&self) -> Result<(), Box<dyn Error>>;
    fn matches(&self, event: &str, params: &str) -> Result<bool, Box<dyn Error>>;
}
pub struct Manifest;
impl module::Manifest for Manifest {
    fn ty() -> config::module::Type { return config::module::Type::Workflow; }
}

pub fn factory<'a>(
    config: &config::Container,
    uses: config::Value,
    with: HashMap<String, config::AnyValue>
) -> Result<Box<dyn Workflow + 'a>, Box<dyn Error>> {
    panic!("Not implemented yet");
}
