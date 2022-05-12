use crate::config;

// trait that object which implement the trait can behave as deplo module.
pub trait Module {
    // module type
    fn ty() -> config::module::Type;
}
