use std::fmt;
use std::path::Path;
use std::collections::HashMap;
use std::error::Error;

use crate::config;

pub mod native; 

pub trait Shell<'a> {
    fn new(config: &'a config::Config) -> Self;
    fn set_cwd<P: AsRef<Path>>(&mut self, dir: Option<&P>) -> Result<(), Box<dyn Error>>;
    fn set_env(&mut self, key: &'a str, val: String) -> Result<(), Box<dyn Error>>;
    fn output_of(&self, args: &Vec<&str>, envs: &HashMap<String, String>) -> Result<String, ShellError>;
    fn exec(&self, args: &Vec<&str>, envs: &HashMap<String, String>, capture: bool) -> Result<String, ShellError>;
    fn eval(&self, code: &str, envs: &HashMap<String, String>, capture: bool) -> Result<String, ShellError> {
        return self.exec(&vec!("sh", "-c", code), envs, capture);
    }
    fn eval_output_of(&self, code: &str, envs: &HashMap<String, String>) -> Result<String, ShellError> {
        return self.output_of(&vec!("sh", "-c", code), envs);
    }
}
pub type Default<'a> = native::Native<'a>;

#[derive(Debug)]
pub enum ShellError {
    ExitStatus {
        status: std::process::ExitStatus
    },
    OtherFailure {
        cause: String
    },
}
impl fmt::Display for ShellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExitStatus { status } => write!(f, "exit status: {}", status),
            Self::OtherFailure { cause } => write!(f, "{}", cause)
        }
    }
}
impl Error for ShellError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

#[macro_export]
macro_rules! macro_ignore_exit_code {
    ($exec:expr) => {
        match $exec {
            Ok(_) => {},
            Err(err) => match err {
                shell::ShellError::ExitStatus{ status:_ } => {},
                _ => return escalate!(Box::new(err))
            }
        }
    };
}

pub use macro_ignore_exit_code as ignore_exit_code;
