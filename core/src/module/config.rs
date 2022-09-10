use serde::{Deserialize, Serialize};

use crate::config;
use crate::util::{escalate};
use crate::shell;

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

#[derive(Serialize, Deserialize)]
pub struct Author {
    pub name: Option<config::Value>,
    pub email: config::Value
}
#[derive(Serialize, Deserialize)]
pub enum EntryPoint {
    Args(Vec<config::Value>),
    Script(config::Value)
}
impl EntryPoint {
    pub fn run(
        &self, shell: S, settings: shell::Settings, cwd: &Option<P>, args: A, envs: E
    ) where
        A: IntoIterator<Item = Arg<'a>>,
        E: IntoIterator<Item = (K, Arg<'a>)>,
        P: shell::ArgTrait,
        S: shell::Shell,
        K: AsRef<OsStr>
    -> Result<String, Box<dyn Error>> {
        Ok(match self {
            Args(a) => return shell.exec(args, envs, cwd, settings)?,
            Script(shell) => return shell.eval(code, shell::sheban_of(code, "bash"), envs, cwd, settings)?
        })
    }
}
#[derive(Serialize, Deserialize)]
pub struct EntryPoints(HashMap<String, EntryPoint>)
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
    pub entrypoints: EntryPoints,
    pub workdir: Option<String>
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
    pub fn run(
        &self, module_type: &str, shell: S, settings: shell::Settings, cwd: &Option<P>, args: A, envs: E
    ) where
        A: IntoIterator<Item = Arg<'a>>,
        E: IntoIterator<Item = (K, Arg<'a>)>,
        P: shell::ArgTrait,
        S: shell::Shell,
        K: AsRef<OsStr>
    -> Result<String, Box<dyn Error>> {
        match self.entrypoints.get(module_type) {
            Some(e) => e.run(shell, settings, cwd, args, envs),
            None => escalate!(Box::new(ModuleError{
                cause: format!("module {} does not have entrypoint for {}", self.name, module_type)
            }))
        }
    }
}