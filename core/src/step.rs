use std::collections::HashMap;
use std::error::Error;

use crate::config;
use crate::module;

pub trait Step {
    fn new(config: &config::Container, params: &HashMap<String, config::AnyValue>) -> Result<Self, Box<dyn Error>> where Self : Sized;
    fn run_step(&self) -> Result<(), Box<dyn Error>>;
}
#[derive(Clone)]
pub struct Module;
impl module::Module for Module {
    fn ty() -> config::module::Type { return config::module::Type::Step; }
}

pub fn factory<'a>(
    config: &config::Container,
    uses: &config::Value,
    with: &Option<HashMap<String, config::AnyValue>>
) -> Result<Box<dyn Step + 'a>, Box<dyn Error>> {
    panic!("Not implemented yet");
}
