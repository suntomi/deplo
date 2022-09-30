use std::error::Error;
use std::fmt;

use crate::config;

#[derive(Debug)]
pub struct ModuleError {
    pub cause: String
}
impl fmt::Display for ModuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl Error for ModuleError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

// trait that object which implement the trait can behave as deplo module.
pub trait Description {
    // module type
    fn ty() -> config::module::Type;
}

