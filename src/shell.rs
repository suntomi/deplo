use std::fmt;
use std::fs;
use std::path::Path;
use std::collections::HashMap;
use std::error::Error;

use crate::config;
use crate::util::escalate;

pub mod native; 

pub trait Shell {
    fn new(config: &config::Container) -> Self;
    fn set_cwd<P: AsRef<Path>>(&mut self, dir: Option<&P>) -> Result<(), Box<dyn Error>>;
    fn set_env(&mut self, key: &str, val: String) -> Result<(), Box<dyn Error>>;
    fn output_of(&self, args: &Vec<&str>, envs: &HashMap<&str, &str>) -> Result<String, ShellError>;
    fn exec(&self, args: &Vec<&str>, envs: &HashMap<&str, &str>, capture: bool) -> Result<String, ShellError>;
    fn eval(&self, code: &str, envs: &HashMap<&str, &str>, capture: bool) -> Result<String, ShellError> {
        return self.exec(&vec!("sh", "-c", code), envs, capture);
    }
    fn eval_output_of(&self, code: &str, envs: &HashMap<&str, &str>) -> Result<String, ShellError> {
        return self.output_of(&vec!("sh", "-c", code), envs);
    }
    fn run_code_or_file(
        &self, code_or_file: &str, envs: &HashMap<&str, &str>
    ) -> Result<(), Box<dyn Error>> {
        let r = match fs::metadata(code_or_file) {
            Ok(_) => self.exec(
                &vec!(code_or_file), envs, false
            ),
            Err(_) => self.eval(
                &format!("echo \'{}\' | bash", code_or_file), envs, false
            )
        };
        match r {
            Ok(_) => Ok(()),
            Err(err) => escalate!(Box::new(err))
        }
    }
}
pub type Default = native::Native;

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
