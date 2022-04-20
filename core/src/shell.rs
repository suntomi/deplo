use std::fmt;
use std::path::Path;
use std::collections::HashMap;
use std::error::Error;
use std::ffi::OsStr;
use std::convert::AsRef;

use crate::config;
use crate::util::{escalate,make_absolute,docker_mount_path,join_vector};

pub mod native;

pub type Arg<'a> = Box<dyn AsRef<OsStr> + 'a>;

#[macro_export]
macro_rules! arg {
    ($x:expr) => {
        Box::new(std::ffi::OsStr::new($x)) as crate::shell::Arg
    }
}

#[macro_export]
macro_rules! args {
    () => (
        std::vec::Vec::new()
    );
    ($($x:expr),+) => (
        vec![$(Box::new(std::ffi::OsStr::new($x)) as crate::shell::Arg),+]
    );
}

#[macro_export]
macro_rules! protected_arg {
    ($x:expr) => {
        Box::new(crate::config::value::Sensitive::new($x)) as crate::shell::Arg
    };
}

pub use arg;
pub use args;
pub use protected_arg;

pub struct Settings {
    capture: bool,
    interactive: bool,
    silent: bool,
}

pub trait Shell {
    fn new(config: &config::Container) -> Self;
    fn set_cwd<P: AsRef<Path>>(&mut self, dir: &Option<P>) -> Result<(), Box<dyn Error>>;
    fn set_env(&mut self, key: &str, val: String) -> Result<(), Box<dyn Error>>;
    fn config(&self) -> &config::Container;
    fn output_of<'a, I, K, V, P>(
        &self, args: &Vec<Arg>, envs: I, cwd: &Option<P>
    ) -> Result<String, ShellError>
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path>;
    fn exec<I, K, V, P>(
        &self, args: &Vec<Arg>, envs: I, cwd: &Option<P>, settings: &Settings
    ) -> Result<String, ShellError>
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path>;
    fn eval<I, K, V, P>(
        &self, code: &str, shell: &Option<String>, envs: I, cwd: &Option<P>, settings: &Settings
    ) -> Result<String, ShellError> 
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path> {
        let sh = shell.as_ref().map_or_else(|| "bash", |v| v.as_str());
        return self.exec(&args!(sh, "-c", code), envs, cwd, settings);
    }
    fn eval_on_container<I, K, V, P>(
        &self, image: &str, code: &str, shell: &Option<String>, envs: I,
        cwd: &Option<P>, mounts: I, settings: &Settings
    ) -> Result<String, Box<dyn Error>>
    where I: IntoIterator<Item = (K, V)> + Clone, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path> {
        let config = self.config().borrow();
        let mut envs_vec: Vec<Arg> = vec![];
        for (k, v) in envs {
            let key = k.as_ref().to_string_lossy();
            let val = v.as_ref().to_string_lossy();
            envs_vec.push(arg!("-e"));
            envs_vec.push(protected_arg!(format!("{k}={v}", k = key, v = val).as_str()));
        }
        let mounts_vec: Vec<Arg> = vec![];
        for (k, v) in mounts {
            let key = k.as_ref().to_string_lossy();
            let val = v.as_ref().to_string_lossy();
            mounts_vec.push(arg!("-e"));
            mounts_vec.push(protected_arg!(format!(
                "{k}:{v}", k = docker_mount_path(&key), v = val
            ).as_str()));
        }
        let repository_mount_path = config.modules.vcs.unwrap().repository_root()?;
        let workdir = match cwd {
            Some(dir) => make_absolute(
                    dir.as_ref(), 
                    &repository_mount_path.clone()
                ).to_string_lossy().to_string(),
            None => repository_mount_path.clone()
        };
        let result = self.exec(&join_vector(vec![
            args!["docker", "run", "--init", "--rm"],
            if settings.interactive { args!["-it"] } else { args![] },
            args!["--workdir", &docker_mount_path(&workdir)],
            envs_vec, mounts_vec,
            // TODO_PATH: use Path to generate path of /var/run/docker.sock (left(host) side)
            args!["-v", &format!("{}:/var/run/docker.sock", docker_mount_path("/var/run/docker.sock"))],
            args!["-v", &format!("{}:{}", docker_mount_path(&repository_mount_path), docker_mount_path(&repository_mount_path))],
            args!["--entrypoint", shell.as_ref().map_or_else(|| "bash", |v| v.as_str())],
            args![image, "-c", code]
        ]), HashMap::<K,V>::new(), &None as &Option<std::path::PathBuf>, &settings)?;
        return Ok(result);
    }
    fn eval_output_of<I, K, V, P>(
        &self, code: &str, shell: &Option<String>, envs: I, cwd: &Option<P>
    ) -> Result<String, ShellError>
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr>, P: AsRef<Path> {
        return self.output_of(&args!(shell.as_ref().map_or_else(|| "bash", |v| v.as_str()), "-c", code), envs, cwd);
    }
    fn detect_os(&self) -> Result<config::job::RunnerOS, Box<dyn Error>> {
        match self.output_of(&args!["uname"], no_env(), no_cwd()) {
            Ok(output) => {
                if output.contains("Darwin") {
                    Ok(config::job::RunnerOS::MacOS)
                } else if output.contains("Linux") {
                    Ok(config::job::RunnerOS::Linux)
                } else if output.contains("Windows") || 
                    output.starts_with("MINGW") || 
                    output.starts_with("MSYS") || 
                    output.starts_with("CYGWIN") {
                    Ok(config::job::RunnerOS::Windows)
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
        self.exec(&args![
            "curl", "-L", url, "-o", output_path
        ], no_env(), no_cwd(), &no_capture())?;
        if executable {
            self.exec(&args![
                "chmod", "+x", output_path
            ], no_env(), no_cwd(), &no_capture())?;
        }
        Ok(())
    }
}
pub type Default = native::Native;

pub fn new_default(config: &config::Container) -> Default {
    return native::Native::new(config);
}

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
pub fn capture() -> Settings {
    return Settings{ capture: true, interactive: false, silent: false };
}
pub fn no_capture() -> Settings {
    return Settings{ capture: false, interactive: false, silent: false };
}
pub fn interactive() -> Settings {
    return Settings{ capture: false, interactive: true, silent: false };
}
pub fn silent() -> Settings {
    return Settings{ capture: true, interactive: false, silent: true };
}