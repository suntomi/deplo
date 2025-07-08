use std::fmt;
use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::AsRef;
use std::error::Error;
use std::ffi::OsStr;
use std::path;

use glob::glob;
use maplit::hashmap;
use regex::{Regex};

use crate::config;
use crate::util::{escalate,make_absolute,docker_mount_path,path_join,join_vector};

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
    ($proc:expr, $($x:expr),+) => {
        $crate::config::value::Synthesize::new($proc, vec![$($crate::arg!($x)),+])
    };
}
pub use synthesize_arg_internal as synthesize_arg;

#[macro_export]
macro_rules! fmtargs_internal {
    ($format:expr) => {
        $crate::config::value::Synthesize::new(
            |v| v[0].value(), vec![$crate::arg!($format)]
        )
    };
    ($format:expr, $($values:expr),*) => {{
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

pub fn mctoa<'a, I, K, V: 'a>(collection: I) -> Vec<(K, Arg<'a>)>
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
    env_inherit: bool,
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
    pub fn inherit_env(&mut self) -> &mut Self {
        self.env_inherit = true;
        self
    }
    pub fn clear_env(&mut self) -> &mut Self {
        self.env_inherit = false;
        self
    }
}

#[derive(Eq, PartialEq, Hash)]
pub enum MountType {
    Bind,
    BindWithTarget(String),
    NamedVolume(String),
    AnonVolume
}
pub struct ContainerMounts<'a>(HashMap<MountType, Vec<&'a config::Value>>);
impl<'a> ContainerMounts<'a> {
    fn resolve_path(pattern: &str) -> Cow<'_,str> {
        // replace ~ to $HOME
        let re = Regex::new(r"^\~(.*)").unwrap();
        match re.captures(pattern) {
            Some(c) => {
                let home = std::env::var("HOME").expect("if you use ~ for cache pattern,$HOME should set");
                let rel = c.get(1).unwrap().as_str();
                let realpath = if rel.starts_with('/') {
                    path_join(vec![home.as_str(), &rel[1..]])
                } else {
                    path_join(vec![home.as_str(), rel])
                };
                Cow::Owned(realpath.to_string_lossy().to_string())
            },
            None => Cow::Borrowed(pattern)
        }
    }
    fn glob(pattern: &str) -> glob::Paths {
        glob(&Self::resolve_path(pattern).to_string()).expect(&format!("invalid glob pattern {}", pattern))
    }
    fn glob2str<'b>(may_path: &'b glob::GlobResult, repository_root: &str) -> String {
        match may_path {
            Ok(p) => Self::to_abs_path(p, repository_root),
            Err(err) => panic!("glob error {}", err)
        }
    }
    fn to_abs_path(path: &path::PathBuf, repository_root: &str) -> String {
        docker_mount_path(make_absolute(path, repository_root).to_string_lossy().to_string().as_str())
    }
    pub fn to_args(&self, repository_root: &'a str) -> Vec<Arg<'a>> {
        join_vector(self.0.iter().map(|(t,patterns)| {
            match t {
                MountType::Bind => if patterns.len() <= 0 {
                    args!["--mount", format!(
                        "type=bind,source={path},target={path}", path = docker_mount_path(repository_root)
                    )]
                } else {
                    join_vector(patterns.iter().map(
                        |pattern| join_vector(Self::glob(pattern.resolve().as_str()).map(
                            |p| args![
                                "--mount",
                                format!("type=bind,source={path},target={path}", path = Self::glob2str(&p, repository_root))
                            ]
                        ).collect())
                    ).collect())
                },
                MountType::BindWithTarget(src) => if patterns.len() != 1 {
                    panic!("if bind has target mount path, only one patterns can be specified but {:?}", patterns);
                } else {
                    args!["--mount", format!(
                        "type=bind,source={src},target={target}",
                        src = docker_mount_path(src), target = patterns[0]
                    )]
                }
                MountType::NamedVolume(v) => join_vector(patterns.iter().map(
                    |pattern| args![
                        "--mount", format!("source={v},target={path}",
                            v = v, path = Self::to_abs_path(
                                &path::PathBuf::from(pattern.resolve()), repository_root
                            )
                        )
                    ]
                ).collect()),
                MountType::AnonVolume => join_vector(patterns.iter().map(
                    |pattern| join_vector(Self::glob(pattern.resolve().as_str()).map(
                        |p| args![
                            "--mount",
                            format!("target={path}", path = Self::glob2str(&p, repository_root))
                        ]
                    ).collect())
                ).collect())
            }
        }).collect())
    }
    fn with_job_and_inputs(
        config: &'a config::Config,
        job: &'a config::job::Job,
        inputs: &'a Option<Vec<config::job::Input>>,
        caches: &'a Option<Vec<config::Value>>
    ) -> ContainerMounts<'a> {
        let mut bind_volumes = vec![];
        let mut anon_volumes = vec![];
        let mut named_volumes = hashmap!{};
        if let Some(ref input_list) = inputs {
            for input in input_list {
                match input {
                    config::job::Input::Path(path) => bind_volumes.push(path),
                    config::job::Input::List{includes, excludes} => {
                        for include in includes {
                            bind_volumes.push(include);
                        }
                        for exclude in excludes {
                            anon_volumes.push(exclude);
                        }
                    }
                }
            }
        }
        match &caches {
            Some(c) => {
                let entries = named_volumes.entry(
                    format!("{}-deplo-local-fallback-cache-volume-{}", config.project_name(), job.name)
                ).or_insert(vec![]);
                for path in c {
                    entries.push(path);
                }
            },
            None => match &job.caches {
                Some(c) => {
                    for (name, cache) in c {
                        let entries = named_volumes.entry(
                            format!("{}-deplo-cache-volume-{}-{}", config.project_name(), job.name, name)
                        ).or_insert(vec![]);
                        for path in &cache.paths {
                            entries.push(path);
                        }
                    }
                },
                None => {}
            }
        }
        let mut map = hashmap!{
            MountType::AnonVolume => anon_volumes,
            MountType::Bind => bind_volumes,
        };
        for (name, entries) in named_volumes {
            map.insert(MountType::NamedVolume(name), entries);
        }
        Self(map)
    }
    pub fn new(config: &'a config::Config, job: &'a config::job::Job) -> ContainerMounts<'a> {
        match &job.runner {
            config::job::Runner::Container{inputs,..} => Self::with_job_and_inputs(config, job, inputs, &None),
            config::job::Runner::Machine{local_fallback,..} => match local_fallback {
                Some(lf) => Self::with_job_and_inputs(config, job, &lf.inputs, &lf.caches),
                None => Self::with_job_and_inputs(config, job, &None, &None)
            }
        }
    }
    pub fn bind<K: AsRef<OsStr>>(
        &mut self, binds: HashMap<K, &'a config::Value>
    ) -> &ContainerMounts<'a> {
        let map = &mut self.0;
        for (k, v) in binds {
            map.insert(MountType::BindWithTarget(k.as_ref().to_string_lossy().to_string()), vec![v]);
        }
        return self
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
    fn eval_on_container<'a, I, K, P>(
        &self, image: &str, code: &str, shell: &Option<String>, envs: I,
        cwd: &Option<P>, mounts: &ContainerMounts<'a>, settings: &Settings
    ) -> Result<String, Box<dyn Error>>
    where 
        I: IntoIterator<Item = (K, Arg<'a>)>,
        K: AsRef<OsStr>, P: ArgTrait
    {
        let config = self.config().borrow();
        let mut envs_vec: Vec<Arg> = vec![];
        for (k, v) in envs {
            let key = k.as_ref().to_string_lossy().to_string();
            envs_vec.push(arg!("-e"));
            envs_vec.push(kv_arg!(arg!(key), v, "="));
        }
        let repository_mount_path = config.modules.vcs().repository_root()?;
        let mounts_vec = mounts.to_args(&repository_mount_path);
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

// tools in windows runner seems to depend on preset environment variables,
// so we have to inherit them because its hard to know which environment variables are required.
#[cfg(windows)]
pub const ENV_INHERIT_DEFAULT: bool = true;
#[cfg(not(windows))]
pub const ENV_INHERIT_DEFAULT: bool = false;

pub fn capture() -> Settings {
    return Settings{ capture: true, interactive: false, silent: false, env_inherit: ENV_INHERIT_DEFAULT, paths: None };
}
pub fn no_capture() -> Settings {
    return Settings{ capture: false, interactive: false, silent: false, env_inherit: ENV_INHERIT_DEFAULT, paths: None };
}
pub fn capture_inherit() -> Settings {
    return Settings{ capture: true, interactive: false, silent: false, env_inherit: true, paths: None };
}
pub fn interactive() -> Settings {
    return Settings{ capture: false, interactive: true, silent: false, env_inherit: ENV_INHERIT_DEFAULT, paths: None };
}
pub fn silent() -> Settings {
    return Settings{ capture: true, interactive: false, silent: true, env_inherit: ENV_INHERIT_DEFAULT, paths: None };
}

pub fn sheban_of<'a>(script: &'a str, fallback: &'a str) -> &'a str {
    let re = Regex::new(r"^#!([^\n]+)").unwrap();
    match re.captures(script) {
        Some(c) => c.get(1).unwrap().as_str(),
        None => fallback
    }
}

mod tests {
    #[allow(unused_imports)]
    use crate::shell::*;
    #[test]
    fn glob_test() {
        for entry in glob::glob("~/*/").unwrap() {
            assert_eq!(entry.is_ok(), true);
            println!("entry = {}", entry.unwrap().to_string_lossy());
        }
    }
    #[test]
    fn large_output_test() {
        let shell = crate::shell::new_default(&crate::config::Config::with(None).unwrap());
        std::fs::write("/tmp/large-text.json", include_str!("../res/test/large-text.json")).unwrap();
        shell.exec(args!(
            "cat", "/tmp/large-text.json"
        ), crate::shell::no_env(), crate::shell::no_cwd(), &crate::shell::capture()).unwrap();
    }
}
