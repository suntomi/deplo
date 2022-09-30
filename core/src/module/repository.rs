use std::fs;

use maplit::hashmap;
use regex::Regex;
use url;

use crate::config;
use crate::module;
use crate::shell;
use crate::util::{escalate};

pub enum Source {
    Std(String),
    Git{ git: String, rev: Option<String>, tag: Option<String> },
    File{ url: String }
}
impl Source {
    fn to_string(&self) -> String {
        match self {
            Std(name) => name.to_string(),
            Git{git, rev, tag} => match rev {
                Some(r) => format!("git:{}#{}", git, r),
                None => match tag {
                    Some(t) => format!("git:{}@{}", git, t),
                    None => panic!("git source but neither rev nor tag specified")
                }
            },
            File{url} => format!("file:{}", url)
        }
    }
}
enum Version {
    Hash(String),
    Tag(String)
};
pub struct Repository<S: shell::Shell = shell::Default> {
    mods: HashMap<String, module::config::Config>,
    shell: S
}
impl<S: shell::Shell> Repository<S> {
    pub fn new(config: &config::Config) -> Self {
        Self {
            mods: hashmap!{},
            shell: S::new(config)
        }
    }
    pub fn get(
        &'a self, config: &config::Config, src: &Source
    ) -> Result<&'a module::config::Config, Box<dyn Error>> {
        self.mods.entry(src.to_string()).or_insert(self.fetch(config, src)?)
    }
    fn fetch(
        &self, config: &config::Config, src: &Source
    ) -> Result<module::config::Config, Box<dyn Error>> {
        let mut module_path = config.deplo_module_root_path()?;
        let (url, ver) = match src {
            Std(name) => {
                let re = Regex::new(r"([^/@]+)/([^/@]+)@([^/@])").unwrap();
                match re.capture(name) {
                    Some(c) => {
                        let (user, name, version) = (
                            c.get(1).unwrap().as_str(),
                            c.get(2).unwrap().as_str(),
                            c.get(3).unwrap().as_str()
                        );
                        module_path.push(user);
                        module_path.push(name);
                        module_path.push(version);
                        (format!("https://github.com/{}/{}", user, name), Version::Tag(version))
                    },
                    None => return escalate!(Box::new(module::ModuleError{
                        cause: format!("{} is not valid standard module path", name)
                    })),
                }
            },
            Git{git, rev, tag} => {
                let url = Url::parse(git)?;
                module_path.push(url.host_str());
                match url.path_segments() {
                    Some(ss) => for c in ss {
                        module_path.push(c);
                    },
                    None => {}
                }
                (git, match rev {
                    Some(r) => {
                        modle_path.push(r);
                        Version::Hash(r)
                    },
                    None => match tag {
                        Some(t) => {
                            modle_path.push(t);
                            Version::Tag(t)
                        },
                        None => return escalate!(Box::new(module::ModuleError{
                            cause: "when module is fetched from git repository, \
                                    either revision or tag should be set to get stable build result".to_string()
                        }));
                    }
                })
            },
            File{url} => return escalate!(Box::new(module::ModuleError{
                cause: format!("donwload tarball from {} does not support", url)
            }));
        };
        let shell_opts = if config.runtime.verbosity > 2 {
            shell::no_capture()
        } else {
            shell::capture()
        };
        match fs::metadata(&module_path) {
            Ok(mata) => if mata.is_dir() {
                log::debug!("module already exists at {:?}", module_path),
            } else {
                panic!("module should already exists at {:?} but its not directory", module_path)
            },
            Err(_) => match ver {
                Version::Hash(h) => {
                    shell.exec(shell::args![
                        "git", "clone", 
                        "-c", format!("remote.origin.fetch=+{hash}:refs/remotes/origin/{hash}", hash = h),
                        "--depth", "1", url, module_path.to_str()
                    ], shell::no_env(), shell::no_cwd(), shell_opts)?
                },
                Version::Tag(t) => {
                    shell.exec(shell::args![
                        "git", "clone", "--depth", "1", "--branch", t, url, module_path.to_str()
                    ], shell::no_env(), shell::no_cwd(), shell_opts)?
                }
            }
        };
        module_path.push("Deplo.Module.toml");
        module::config::Config::with(module_path.to_string_lossy().to_string())
    }
}