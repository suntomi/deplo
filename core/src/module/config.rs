use serde::{Deserialize, Serialize};

use serde_json;
use toml;

use crate::config;
use crate::util::{escalate};
use crate::shell;

#[derive(Serialize, Deserialize)]
pub struct Author {
    pub name: Option<config::Value>,
    pub email: config::Value
}
#[derive(Serialize, Deserialize, Hash)]
pub enum EntryPointType {
    #[serde(rename = "ci")]
    CI,
    #[serde(rename = "vcs")]
    VCS,
    #[serde(rename = "step")]
    Step,
    #[serde(rename = "workflow")]
    Workflow,
    #[serde(rename = "job")]
    Job,
    #[serde(rename = "secret")]
    Secret,
};
#[derive(Serialize, Deserialize)]
pub enum EntryPoint(HashMap<config::job::RunnerOS, Vec<config::Value>>);
impl EntryPoint {
    pub fn run<'a>(
        &'a self, shell: S, settings: shell::Settings, cwd: &Option<P>, args: A, envs: E
    ) where
        A: IntoIterator<Item = Arg<'a>>,
        E: IntoIterator<Item = (K, Arg<'a>)>,
        P: shell::ArgTrait,
        S: shell::Shell,
        K: AsRef<OsStr>
    -> Result<String, Box<dyn Error>> {
        let os = shell.detect_os()?;
        match self.0.get(os) {
            Some(ep_args) => {
                let mut cmd = ep_args.iter().map(|c| shell::arg!(c)).collect();
                for a in args.into_iter() {
                    cmd.push(a)
                }
                shell.exec(cmd, envs, cwd, settings)
            },
            None => escalate!(Box::new(module::ModuleError{
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
};
pub type ConfigVersion = u64;
fn default_config_version() -> ConfigVersion { 1 }
#[derive(Serialize, Deserialize)]
pub struct Config {
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
impl Config {
    pub fn with(path: &str) -> Self {
        let src = config::source::Source::File(path);
        let ret = src::load_as::<Self>();
        if ret.workdir.is_none() {
            ret.workdir = Some(path.to_string())
        }
        ret
    }
    pub fn embed_option_to_env(
        &self, envs: E, option: &Option<HashMap<String, config::AnyValue>>
    ) where 
        E: IntoIterator<Item = (K, Arg<'a>)>,
        F: IntoIterator<Item = (L, Arg<'a>)>,
        K: AsRef<OsStr>,
        L: AsRef<OsStr>,
    -> F {
        match option {
            Some(o) => {
                let (opt_format, opt_str) = match self.option_format {
                    Some(f) => match f {
                        Json => ("json", serde_json::to_string(o)?),
                        Toml => ("toml", toml::to_string(o)?)
                    },
                    None => ("json", serde_json::to_string(o)?),
                };
                let mut new_envs = hashmap!{};
                for (k,v) in envs.into_iter() {
                    new_envs.insert(k, v)
                }
                new_envs.insert("DEPLO_MODULE_OPTION_STRING", shell::protected_arg!(opt_str));
                new_envs.insert("DEPLO_MODULE_OPTION_STRING_FORMAT", shell::arg!(opt_format));
                new_envs
            },
            None => envs
        }
    }
    pub fn run(
        &self, ep_type: EntryPointType, shell: S, settings: shell::Settings, 
        cwd: &Option<P>, args: A, envs: E, 
        option: &Option<HashMap<String, config::AnyValue>>
    ) where
        A: IntoIterator<Item = Arg<'a>>,
        E: IntoIterator<Item = (K, Arg<'a>)>,
        P: shell::ArgTrait,
        S: shell::Shell,
        K: AsRef<OsStr>
    -> Result<String, Box<dyn Error>> {
        match self.entrypoints.get(ep_type) {
            Some(e) => e.run(shell, settings, cwd, args, self.embed_option_to_env(envs, option)),
            None => escalate!(Box::new(module::ModuleError{
                cause: format!("module {} does not have entrypoint for {}", self.name, module_type)
            }))
        }
    }
}