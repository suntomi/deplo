use std::fmt;
use std::fs;
use std::path::Path;
use std::collections::HashMap;
use std::error::Error;
use std::ffi::OsStr;
use std::convert::AsRef;

use crate::config;
use crate::util::escalate;

pub mod native; 

pub trait Shell {
    fn new(config: &config::Container) -> Self;
    fn set_cwd<P: AsRef<Path>>(&mut self, dir: Option<&P>) -> Result<(), Box<dyn Error>>;
    fn set_env(&mut self, key: &str, val: String) -> Result<(), Box<dyn Error>>;
    fn output_of<I, K, V>(
        &self, args: &Vec<&str>, envs: I
    ) -> Result<String, ShellError>
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>;
    fn exec<I, K, V>(
        &self, args: &Vec<&str>, envs: I, capture: bool
    ) -> Result<String, ShellError>
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>;
    fn eval<I, K, V>(
        &self, code: &str, envs: I, capture: bool
    ) -> Result<String, ShellError> 
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr> {
        return self.exec(&vec!("bash", "-c", code), envs, capture);
    }
    fn eval_output_of<I, K, V>(
        &self, code: &str, envs: I
    ) -> Result<String, ShellError>
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr> {
        return self.output_of(&vec!("bash", "-c", code), envs);
    }
    fn run_code_or_file<I, K, V>(
        &self, code_or_file: &str, envs: I
    ) -> Result<(), Box<dyn Error>>
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr> {
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
        status: std::process::ExitStatus,
        cmd: String,
        stderr: String
    },
    OtherFailure {
        cmd: String,
        cause: String
    },
}
impl fmt::Display for ShellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExitStatus { status, stderr, cmd } => {
                write!(f, "cmd:{}, exit status:{}, stedrr:{}", cmd, status, stderr)
            },
            Self::OtherFailure { cmd, cause } => write!(f, "cmd:{}, err:{}", cmd, cause)
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
                shell::ShellError::ExitStatus{ status:_, stderr:_, cmd:_ } => {},
                _ => return escalate!(Box::new(err))
            }
        }
    };
}

pub use macro_ignore_exit_code as ignore_exit_code;
pub fn no_env() -> HashMap<String, String> {
    return HashMap::new()
}
