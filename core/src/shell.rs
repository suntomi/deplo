use std::fmt;
use std::borrow::Cow;
use std::collections::HashMap;
use std::error::Error;
use std::ffi::OsStr;
use std::convert::AsRef;

use crate::config;
use crate::util::{escalate,make_absolute,docker_mount_path,join_vector};

pub mod native;

pub trait ArgTrait {
    fn value(&self) -> String;
    fn view(&self) -> Cow<'_, str>;
}
pub type Arg<'a> = Box<dyn ArgTrait + 'a>;

impl ArgTrait for &OsStr {
    fn value(&self) -> String {
        self.to_string_lossy().to_string()
    }
    fn view(&self) -> Cow<'_, str> {
        self.to_string_lossy()
    }
}
impl ArgTrait for &str {
    fn value(&self) -> String {
        self.to_string()
    }
    fn view(&self) -> Cow<'_, str> {
        Cow::Borrowed(self)
    }
}
impl ArgTrait for String {
    fn value(&self) -> String {
        self.clone()
    }
    fn view(&self) -> Cow<'_, str> {
        Cow::Borrowed(self)
    }
}
impl ArgTrait for &String {
    fn value(&self) -> String {
        self.to_string()
    }
    fn view(&self) -> Cow<'_, str> {
        Cow::Borrowed(self)
    }
}

pub trait Inc {
    fn inc(&mut self,_void:()) -> Self;
}
impl Inc for usize {
    fn inc(&mut self,_void:()) -> Self {
        let prev = *self;
        *self = prev + 1;
        return prev;
    }
}

#[macro_export]
macro_rules! arg {
    ($x:expr) => {
        Box::new($x) as crate::shell::Arg
    }
}

#[macro_export]
macro_rules! args {
    () => (
        std::vec::Vec::new()
    );
    ($($x:expr),+) => (
        vec![$(Box::new($x) as crate::shell::Arg),+]
    );
}

#[macro_export]
macro_rules! protected_arg {
    ($x:expr) => {
        Box::new(crate::config::value::Sensitive::new($x)) as crate::shell::Arg
    };
}

#[macro_export]
macro_rules! kv_arg {
    ($k:expr,$v:expr,$seps:expr) => {
        Box::new(crate::config::value::KeyValue::new($k,$v,$seps)) as crate::shell::Arg
    };
}

#[macro_export]
macro_rules! synthesize_arg_internal {
    ($proc: expr, $($values: expr),+) => {
        $crate::config::value::Synthesize::new($proc, vec![$($crate::arg!($values)),+])
    };
}
pub use synthesize_arg_internal as synthesize_arg;

#[macro_export]
macro_rules! fmtargs_internal {
    ($format: expr) => {
        $crate::config::value::Synthesize::new(
            |v| v[0].value(), vec![$crate::arg!($format)]
        )
    };
    ($format: expr, $($values: expr),*) => {{
        $crate::config::value::Synthesize::new(|v| {
            let mut tmpl = formatx::Template::new(&v[0]).expect(&format!("format parse error {}", v[0]));
            for e in &v[1..] {
                tmpl.replace_positional(e);
            }
            tmpl.text().expect(&format!("runtime format error {}", v[0]))
        }, vec![$crate::arg!($format),$($crate::arg!($values)),*])
    }};
}
pub use fmtargs_internal as fmtargs;

#[macro_export]
#[doc(hidden)]
macro_rules! format_internal_args {
    ($args: expr, $value: expr) => {
        $args.push($crate::arg!($value));
    };
    ($args: expr, $value: expr, $($values: tt)*) => {
        $args.push($crate::arg!($value));
        $crate::format_internal_args!($args, $($values)*);
    };
}

pub fn ctoa<'a, I, K, V: 'a>(collection: I) -> Vec<(K, Arg<'a>)>
where 
    I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: ArgTrait {
    let mut h = vec![];
    for (k,v) in collection.into_iter() {
        h.push((k, arg!(v)));
    }
    return h;
}

pub use arg;
pub use args;
pub use protected_arg;

#[derive(Clone)]
pub struct Settings {
    capture: bool,
    interactive: bool,
    silent: bool,
    paths: Option<Vec<String>>
}
impl Settings {
    pub fn paths(&mut self, paths: Vec<String>) -> &mut Self {
        match &mut self.paths {
            Some(p) => p.extend(paths),
            None => self.paths = Some(paths)
        };
        self
    }
}

pub trait Shell {
    fn new(config: &config::Container) -> Self;
    fn set_cwd<P: ArgTrait>(&mut self, dir: &Option<P>) -> Result<(), Box<dyn Error>>;
    fn set_env(&mut self, key: &str, val: String) -> Result<(), Box<dyn Error>>;
    fn config(&self) -> &config::Container;

    fn output_of<'a, I, J, K, P>(
        &self, args: I, envs: J, cwd: &Option<P>
    ) -> Result<String, ShellError> 
    where 
        I: IntoIterator<Item = Arg<'a>>,
        J: IntoIterator<Item = (K, Arg<'a>)>,
        K: AsRef<OsStr>, P: ArgTrait;

    fn exec<'a, I, J, K, P>(
        &self, args: I, envs: J, cwd: &Option<P>, settings: &Settings
    ) -> Result<String, ShellError> 
    where 
        I: IntoIterator<Item = Arg<'a>>,
        J: IntoIterator<Item = (K, Arg<'a>)>,
        K: AsRef<OsStr>, P: ArgTrait;

    fn eval<'a, I, K, P>(
        &self, code: &'a str, shell: &'a Option<String>, envs: I, cwd: &'a Option<P>, settings: &'a Settings
    ) -> Result<String, ShellError> 
    where 
        I: IntoIterator<Item = (K, Arg<'a>)>,
        K: AsRef<OsStr>, P: ArgTrait
    {
        let sh = shell.as_ref().map_or_else(|| "bash", |v| v.as_str());
        return self.exec(args!(sh, "-c", code), envs, cwd, settings);
    }
    fn eval_on_container<'a, I, J, K, L, P>(
        &self, image: &str, code: &str, shell: &Option<String>, envs: I,
        cwd: &Option<P>, mounts: J, settings: &Settings
    ) -> Result<String, Box<dyn Error>>
    where 
        I: IntoIterator<Item = (K, Arg<'a>)>,
        J: IntoIterator<Item = (L, Arg<'a>)>,
        K: AsRef<OsStr>, L: AsRef<OsStr>, P: ArgTrait 
    {
        let config = self.config().borrow();
        let mut envs_vec: Vec<Arg> = vec![];
        for (k, v) in envs {
            let key = k.as_ref().to_string_lossy().to_string();
            envs_vec.push(arg!("-e"));
            envs_vec.push(kv_arg!(arg!(key), v, "="));
        }
        let mut mounts_vec: Vec<Arg> = vec![];
        for (k, v) in mounts {
            let key = k.as_ref().to_string_lossy();
            let val = v.value();
            mounts_vec.push(arg!("-v"));
            mounts_vec.push(arg!(format!(
                "{k}:{v}", k = docker_mount_path(&key), v = val
            )));
        }
        let repository_mount_path = config.modules.vcs().repository_root()?;
        let workdir = match cwd {
            Some(dir) => make_absolute(
                    &dir.value(),
                    &repository_mount_path.clone()
                ).to_string_lossy().to_string(),
            None => repository_mount_path.clone()
        };
        let result = self.exec(join_vector(vec![
            args!["docker", "run", "--init", "--rm"],
            if settings.interactive { args!["-it"] } else { args![] },
            args!["--workdir", docker_mount_path(&workdir)],
            envs_vec, mounts_vec,
            // TODO_PATH: use Path to generate path of /var/run/docker.sock (left(host) side)
            args!["-v", format!("{}:/var/run/docker.sock", docker_mount_path("/var/run/docker.sock"))],
            args!["-v", format!("{}:{}", docker_mount_path(&repository_mount_path), docker_mount_path(&repository_mount_path))],
            args!["--entrypoint", shell.as_ref().map_or_else(|| "bash", |v| v.as_str())],
            args![image, "-c", code]
        ]), HashMap::<K,Arg<'a>>::new(), no_cwd(), &settings)?;
        return Ok(result);
    }
    fn eval_output_of<'a, I, K, P>(
        &self, code: &'a str, shell: &'a Option<String>, envs: I, cwd: &'a Option<P>
    ) -> Result<String, ShellError>
    where I: IntoIterator<Item = (K, Arg<'a>)>, K: AsRef<OsStr>,P: ArgTrait {
        return self.output_of(args![shell.as_ref().map_or_else(|| "bash", |v| v.as_str()), "-c", code], envs, cwd);
    }
    fn detect_os(&self) -> Result<config::job::RunnerOS, Box<dyn Error>> {
        match self.output_of(args!["uname"], no_env(), no_cwd()) {
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
        self.exec(args![
            "curl", "-L", url, "-o", output_path
        ], no_env(), no_cwd(), &no_capture())?;
        if executable {
            self.exec(args![
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
pub fn no_env<'a>() -> HashMap<String, Arg<'a>> {
    return HashMap::new()
}
pub fn no_cwd<'a>() -> &'a Option<String> {
    return &None;
}
pub fn default<'a>() -> &'a Option<String> {
    return &None;
}
pub fn inherit_env() -> HashMap<String, String> {
    return std::env::vars().collect();
}
pub fn capture() -> Settings {
    return Settings{ capture: true, interactive: false, silent: false, paths: None };
}
pub fn no_capture() -> Settings {
    return Settings{ capture: false, interactive: false, silent: false, paths: None };
}
pub fn interactive() -> Settings {
    return Settings{ capture: false, interactive: true, silent: false, paths: None };
}
pub fn silent() -> Settings {
    return Settings{ capture: true, interactive: false, silent: true, paths: None };
}