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
    fn output_of<I, K, V, P>(
        &self, args: &Vec<&str>, envs: I, cwd: Option<&P>
    ) -> Result<String, ShellError>
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path>;
    fn exec<I, K, V, P>(
        &self, args: &Vec<&str>, envs: I, cwd: Option<&P>, capture: bool
    ) -> Result<String, ShellError>
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path>;
    fn detect_os(&self) -> Result<config::RunnerOS, Box<dyn Error>> {
        match self.eval_output_of("uname", no_env(), no_cwd()) {
            Ok(output) => {
                if output.contains("Darwin") {
                    Ok(config::RunnerOS::MacOS)
                } else if output.contains("Linux") {
                    Ok(config::RunnerOS::Linux)
                } else if output.contains("Windows") {
                    Ok(config::RunnerOS::Windows)
                } else {
                    escalate!(Box::new(ShellError::OtherFailure{ 
                        cmd: "uname".to_string(), 
                        cause: format!("Unsupported OS: {}", output) 
                    }))
                }
            },
            Err(err) => Err(Box::new(err))
        }
    }
    fn eval<I, K, V, P>(
        &self, code: &str, envs: I, cwd: Option<&P>, capture: bool
    ) -> Result<String, ShellError> 
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path> {
        return self.exec(&vec!("bash", "-c", code), envs, cwd, capture);
    }
    fn eval_on_container<I, K, V, P>(
        &self, image: &str, code: &str, envs: I, cwd: Option<&P>, capture: bool
    ) -> Result<String, ShellError>
    where I: IntoIterator<Item = (K, V)> + Clone, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path> {
        let envs_vec: Vec<String> = envs.clone().into_iter().map(|(k,_)| {
            return vec!["-e".to_string(), format!("{k}=${k}", k = k.as_ref().to_string_lossy())]
        }).collect::<Vec<Vec<String>>>().concat();
        return self.exec(&vec![
            vec!["docker", "run", "--rm", "-ti"],
            if cwd.is_none() { vec![] } else { vec!["--workdir", cwd.unwrap().as_ref().to_str().unwrap()] },
            envs_vec.iter().map(|s| s.as_ref()).collect::<Vec<&str>>(),
            vec!["-v", "/var/run/docker.sock:/var/run/docker.sock"],
            vec![image, "bash", "-c", code]
        ].concat(), envs, cwd, capture);
    }
    fn eval_output_of<I, K, V, P>(
        &self, code: &str, envs: I, cwd: Option<&P>
    ) -> Result<String, ShellError>
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path> {
        return self.output_of(&vec!("bash", "-c", code), envs, cwd);
    }
    fn run_code_or_file<I, K, V, P>(
        &self, code_or_file: &str, envs: I, cwd: Option<&P>
    ) -> Result<(), Box<dyn Error>>
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path> {
        let r = match fs::metadata(code_or_file) {
            Ok(_) => self.exec(
                &vec!(code_or_file), envs, cwd, false
            ),
            Err(_) => self.eval(
                &format!("echo \'{}\' | bash", code_or_file), envs, cwd, false
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
                shell::ShellError::ExitStatus{..} => {},
                _ => return escalate!(Box::new(err))
            }
        }
    };
}

pub use macro_ignore_exit_code as ignore_exit_code;
pub fn no_env() -> HashMap<String, String> {
    return HashMap::new()
}
pub fn no_cwd() -> Option<&'static Box<Path>> {
    let none: Option<&Box<Path>> = None;
    return none;
}
pub fn inherit_and(envs: &HashMap<String, String>) -> HashMap<String, String> {
    let mut new_envs: HashMap<String, String> = std::env::vars().collect();
    for (k, v) in envs {
        new_envs.insert(k.to_string(), v.to_string());
    }
    return new_envs;
}