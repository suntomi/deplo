use std::fmt;
use std::path::Path;
use std::collections::HashMap;
use std::error::Error;
use std::ffi::OsStr;
use std::convert::AsRef;

use crate::config;
use crate::util::{escalate,make_absolute};

pub mod native;

pub trait Shell {
    fn new(config: &config::Container) -> Self;
    fn set_cwd<P: AsRef<Path>>(&mut self, dir: &Option<P>) -> Result<(), Box<dyn Error>>;
    fn set_env(&mut self, key: &str, val: String) -> Result<(), Box<dyn Error>>;
    fn config(&self) -> &config::Container;
    fn output_of<I, K, V, P>(
        &self, args: &Vec<&str>, envs: I, cwd: &Option<P>
    ) -> Result<String, ShellError>
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path>;
    fn exec<I, K, V, P>(
        &self, args: &Vec<&str>, envs: I, cwd: &Option<P>, capture: bool
    ) -> Result<String, ShellError>
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path>;
    fn eval<I, K, V, P>(
        &self, code: &str, shell: &Option<String>, envs: I, cwd: &Option<P>, capture: bool
    ) -> Result<String, ShellError> 
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path> {
        return self.exec(&vec!(shell.as_ref().map_or_else(|| "bash", |v| v.as_str()), "-c", code), envs, cwd, capture);
    }
    fn eval_on_container<I, K, V, P>(
        &self, image: &str, code: &str, shell: &Option<String>, envs: I, cwd: &Option<P>, 
        mounts: &HashMap<&str, &str>, capture: bool
    ) -> Result<String, Box<dyn Error>>
    where I: IntoIterator<Item = (K, V)> + Clone, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path> {
        let config = self.config().borrow();
        let envs_vec: Vec<String> = envs.clone().into_iter().map(|(k,v)| {
            let key = k.as_ref().to_string_lossy();
            let val = v.as_ref().to_string_lossy();
            return vec!["-e".to_string(), format!("{k}={v}", k = key, v = val)]
        }).collect::<Vec<Vec<String>>>().concat();
        let mounts_vec: Vec<String> = mounts.iter().map(|(k,v)| {
            return vec!["-v".to_string(), format!("{k}:{v}", k = k, v = v)]
        }).collect::<Vec<Vec<String>>>().concat();
        let repository_mount_path = config.vcs_service()?.repository_root()?;
        let workdir = match cwd {
            Some(dir) => make_absolute(dir.as_ref(), &repository_mount_path.clone()).to_string_lossy().to_string(),
            None => repository_mount_path.clone()
        };
        let result = self.exec(&vec![
            vec!["docker", "run", "--rm"],
            vec!["--workdir", &workdir],
            envs_vec.iter().map(|s| s.as_ref()).collect::<Vec<&str>>(),
            mounts_vec.iter().map(|s| s.as_ref()).collect::<Vec<&str>>(),
            // TODO_PATH: use Path to generate path of /var/run/docker.sock (left(host) side)
            vec!["-v", "/var/run/docker.sock:/var/run/docker.sock"],
            vec!["-v", &format!("{}:{}", &repository_mount_path, &repository_mount_path)],
            vec![image, shell.as_ref().map_or_else(|| "bash", |v| v.as_str()), "-c", code]
        ].concat(), envs, cwd, capture)?;
        return Ok(result);
    }
    fn eval_output_of<I, K, V, P>(
        &self, code: &str, shell: &Option<String>, envs: I, cwd: &Option<P>
    ) -> Result<String, ShellError>
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path> {
        return self.output_of(&vec!(shell.as_ref().map_or_else(|| "bash", |v| v.as_str()), "-c", code), envs, cwd);
    }
    fn detect_os(&self) -> Result<config::RunnerOS, Box<dyn Error>> {
        match self.eval_output_of("uname", default(), no_env(), no_cwd()) {
            Ok(output) => {
                if output.contains("Darwin") {
                    Ok(config::RunnerOS::MacOS)
                } else if output.contains("Linux") {
                    Ok(config::RunnerOS::Linux)
                } else if output.contains("Windows") || 
                    output.starts_with("MINGW") || 
                    output.starts_with("MSYS") || 
                    output.starts_with("CYGWIN") {
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
    fn download(&self, url: &str, output_path: &str, executable: bool) -> Result<(), Box<dyn Error>> {
        self.exec(&vec![
            "curl", "-L", url, "-o", output_path
        ], no_env(), no_cwd(), false)?;
        if executable {
            self.exec(&vec![
                "chmod", "+x", output_path
            ], no_env(), no_cwd(), false)?;
        }
        Ok(())
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
                write!(f, "cmd:{}, {}, stedrr:{}", cmd, status, stderr)
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
pub fn no_cwd<'a>() -> &'a Option<Box<Path>> {
    return &None;
}
pub fn default<'a>() -> &'a Option<String> {
    return &None;
}
pub fn inherit_env() -> HashMap<String, String> {
    return std::env::vars().collect();
}