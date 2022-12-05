use std::collections::HashMap;
use std::error::Error;
use std::ffi::OsStr;
use std::fmt;
use std::path::Path;

use maplit::hashmap;
use serde::{Deserialize, Serialize};
use serde_json;
use toml;

use crate::config;
use crate::util::{escalate};
use crate::shell;

pub mod repos;

#[derive(Debug)]
pub struct ModuleError {
    pub cause: String
}
impl fmt::Display for ModuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl Error for ModuleError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

// trait that object which implement the trait can behave as deplo module.
pub trait Description {
    // module type
    fn ty() -> config::module::Type;
}

#[derive(Serialize, Deserialize)]
pub struct Author {
    pub name: Option<config::Value>,
    pub email: config::Value
}
pub type EntryPointType = config::module::Type;
#[derive(Serialize, Deserialize)]
pub struct EntryPoint(HashMap<config::job::RunnerOS, Vec<config::Value>>);
impl EntryPoint {
    pub fn run<'a,A,E,P,S,K>(
        &'a self, ty: EntryPointType, shell: &S, settings: &shell::Settings,
        cwd: &Option<P>, args: A, envs: E
    ) -> Result<String, Box<dyn Error>> 
    where
        A: IntoIterator<Item = shell::Arg<'a>>,
        E: IntoIterator<Item = (K, shell::Arg<'a>)>,
        P: shell::ArgTrait,
        S: shell::Shell,
        K: AsRef<OsStr>
    {
        let os = shell.detect_os()?;
        match self.0.get(&os) {
            Some(ep_args) => {
                let mut cmd: Vec<shell::Arg<'a>> = ep_args.iter().map(|c| shell::arg!(c)).collect();
                cmd.push(shell::arg!(ty.to_string()));
                for a in args.into_iter() {
                    cmd.push(a)
                }
                let r = shell.exec(cmd, envs, cwd, &settings)?;
                Ok(r)
            },
            None => escalate!(Box::new(ModuleError{
                cause: format!("this entrypoint does not support os type '{}'", os)
            }))
        }
    }
}
#[derive(Serialize, Deserialize)]
pub enum OptionFormat {
    #[serde(rename = "json")]
    Json,
    #[serde(rename = "toml")]
    Toml,
}
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum Source {
    Std(config::Value),
    Git{ git: config::Value, rev: Option<config::Value>, tag: Option<config::Value> },
    Package{ url: config::Value },
    Local{ path: config::Value },
}
impl Source {
    pub fn to_string(&self) -> String {
        match self {
            Self::Std(name) => name.to_string(),
            Self::Git{git: repo_url, rev, tag} => match rev {
                Some(r) => format!("git:{}#{}", repo_url, r),
                None => match tag {
                    Some(t) => format!("git:{}@{}", repo_url, t),
                    None => panic!("git source but neither rev nor tag specified")
                }
            },
            Self::Package{url} => format!("package:{}", url),
            Self::Local{path} => format!("local:{}", path)
        }
    }
}
pub type ConfigVersion = u64;
fn default_config_version() -> ConfigVersion { 1 }
#[derive(Serialize, Deserialize)]
pub struct Module {
    // config that loads from config file
    #[serde(default = "default_config_version")]
    pub config_version: ConfigVersion,
    pub version: String,
    pub name: config::Value,
    pub author: Author,
    pub entrypoints: HashMap<EntryPointType, EntryPoint>,
    pub workdir: Option<String>,
    pub option_format: Option<OptionFormat>
}
impl Module {
    pub fn with(path: &str) -> Result<Self, Box<dyn Error>> {
        let src = config::source::Source::File(path);
        let mut ret = src.load_as::<Self>()?;
        if ret.workdir.is_none() {
            let p = Path::new(path);
            ret.workdir = Some(match p.parent() {
                Some(v) => v.to_string_lossy().to_string(),
                None => "/".to_string()
            })
        } else {
            let relpath = ret.workdir.unwrap();
            let p = Path::new(path);
            let wd = p.parent().expect(&format!("parent should exists for path {}", path)).join(relpath);
            ret.workdir = Some(wd.to_string_lossy().to_string());
        }
        Ok(ret)
    }
    pub fn embed_option_to_env<'a,E,K>(
        &self, envs: E, option: &Option<HashMap<String, config::AnyValue>>
    ) -> Result<HashMap<String, shell::Arg<'a>>, Box<dyn Error>>
    where 
        E: IntoIterator<Item = (K, shell::Arg<'a>)>,
        K: AsRef<OsStr>
    {
        let mut new_envs = hashmap!{};
        for (k,v) in envs.into_iter() {
            new_envs.insert(k.as_ref().to_string_lossy().to_string(), v);
        }
        match option {
            Some(o) => {
                let (opt_format, opt_str) = match &self.option_format {
                    Some(f) => match f {
                        OptionFormat::Json => ("json", serde_json::to_string(o)?),
                        OptionFormat::Toml => ("toml", toml::to_string(o)?)
                    },
                    None => ("json", serde_json::to_string(o)?),
                };
                new_envs.insert(
                    "DEPLO_MODULE_OPTION_STRING".to_string(),
                    shell::protected_arg!(&opt_str)
                );
                new_envs.insert(
                    "DEPLO_MODULE_OPTION_STRING_FORMAT".to_string(),
                    shell::arg!(opt_format)
                );
            },
            None => {}
        }
        Ok(new_envs)
    }
    pub fn run<'a,A,E,S,K>(
        &'a self, ep: EntryPointType, shell: &S, settings: &shell::Settings, 
        args: A, envs: E, option: &Option<HashMap<String, config::AnyValue>>
    ) -> Result<String, Box<dyn Error>>
    where
        A: IntoIterator<Item = shell::Arg<'a>>,
        E: IntoIterator<Item = (K, shell::Arg<'a>)>,
        S: shell::Shell,
        K: AsRef<OsStr>
    {
        match self.entrypoints.get(&ep) {
            Some(e) => e.run(
                ep, shell, settings, &self.workdir, args,
                self.embed_option_to_env(envs, option)?
            ),
            None => escalate!(Box::new(ModuleError{
                cause: format!("module {} does not have entrypoint for {}", self.name, ep)
            }))
        }
    }
}

pub fn empty_env<'a>() -> HashMap<String, shell::Arg<'a>> {
    HashMap::new()
}
