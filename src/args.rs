use std::error::Error;
use std::fmt;
use std::result::Result;

pub mod clap;

pub trait Args : Sized {
    fn create() -> Result<Self, Box<dyn Error>>;
    fn subcommand(&self) -> Option<(&str, Self)>;
    fn values_of(&self, name: &str) -> Option<Vec<&str>>;
    fn value_of(&self, name: &str) -> Option<&str> {
        match self.values_of(name) {
            Some(v) => Some(v[0]),
            None => None
        }
    }
    fn occurence_of(&self, name: &str) -> u64 {
        match self.values_of(name) {
            Some(v) => v.len() as u64,
            None => 0
        }
    }
}

pub type Default<'a> = clap::Clap<'a>;
    
#[derive(Debug)]
pub struct ArgsError {
    pub cause: String
}
impl fmt::Display for ArgsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl Error for ArgsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

pub fn create<'a>() -> Result<Default<'a>, Box<dyn Error>> {
    return Default::<'a>::create();
}