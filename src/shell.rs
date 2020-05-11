use std::fmt;
use std::collections::HashMap;
use std::error::Error;

use crate::config;

pub mod native; 

pub trait Shell<'a> {
    fn new(config: &'a config::Config) -> Self;
    fn output_of(&self, args: &Vec<&str>, envs: &HashMap<String, String>) -> Result<String, Box<dyn Error>>;
    fn exec(&self, args: &Vec<&str>, envs: &HashMap<String, String>) -> Result<(), Box<dyn Error>>;
}
pub type Default<'a> = native::Native<'a>;

#[derive(Debug)]
pub struct ShellError {
    cause: String
}
impl fmt::Display for ShellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl Error for ShellError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}
