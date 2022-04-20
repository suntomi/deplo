use std::error::Error;

use crate::config;

// trait that object which implement the trait can behave as deplo module.
pub trait Module {
    // prepare module, once this is called, all method of the object should fully work
    fn prepare(&self, reinit: bool) -> Result<(), Box<dyn Error>>;
}
pub trait Manifest {
    fn ty() -> config::module::Type;
}