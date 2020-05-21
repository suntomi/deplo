use std::fmt;
use std::path::Path;
use std::collections::HashMap;
use std::error::Error;

use crate::config;

pub mod native; 

pub trait Shell<'a> {
    fn new(config: &'a config::Config) -> Self;
    fn set_cwd<P: AsRef<Path>>(&mut self, dir: P) -> Result<(), Box<dyn Error>>;
    fn output_of(&self, args: &Vec<&str>, envs: &HashMap<String, String>) -> Result<String, Box<dyn Error>>;
    fn exec(&self, args: &Vec<&str>, envs: &HashMap<String, String>, capture: bool) -> Result<String, Box<dyn Error>>;
    fn eval(&self, code: &str, envs: &HashMap<String, String>, capture: bool) -> Result<String, Box<dyn Error>> {
        return self.exec(&vec!("sh", "-c", code), envs, capture);
    }
    fn eval_output_of(&self, code: &str, envs: &HashMap<String, String>) -> Result<String, Box<dyn Error>> {
        return self.output_of(&vec!("sh", "-c", code), envs);
    }
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
