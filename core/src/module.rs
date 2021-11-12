use std::error::Error;

// trait that object which implement the trait can behave as deplo module.
pub trait Module {
    // prepare module, once this is called, all method of the object should fully work
    fn prepare(&self, reinit: bool) -> Result<(), Box<dyn Error>>;
}