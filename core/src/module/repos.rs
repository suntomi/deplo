use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

use maplit::hashmap;
use regex::Regex;
use url;

use crate::config;
use crate::module;
use crate::shell;
use crate::util::{escalate};

pub const DEPLO_MODULE_CONFIG_FILE_NAME: &'static str = "Deplo.Module.toml";

enum Version {
    Hash(String),
    Tag(String)
}
pub struct Repository<S: shell::Shell = shell::Default> {
    mods: HashMap<String, module::Module>,
    shell: S
}
impl<S: shell::Shell> Repository<S> {
    pub fn new(config: &config::Container) -> Self {
        Self {
            mods: hashmap!{},
            shell: S::new(config)
        }
    }
    pub fn get<'a>(&'a self, key: &str) -> &'a module::Module {
        self.mods.get(key).expect(&format!("{} not found", key))
    }
    pub fn get_by_src<'a>(&'a self, src: &module::Source) -> &'a module::Module {
        let key = src.to_string();
        self.get(&key)
    }
    pub fn load(
        &mut self, config: &config::Config, src: &module::Source
    ) -> Result<String, Box<dyn Error>> {
        self.mods.insert(src.to_string(), Self::fetch(config, src, &self.shell)?);
        Ok(src.to_string())
    }
    fn fetch(
        config: &config::Config, src: &module::Source, shell: &S
    ) -> Result<module::Module, Box<dyn Error>> {
        let mut module_path = config.deplo_module_root_path()?;
        let (url, ver) = match src {
            module::Source::Std(name) => {
                let re = Regex::new(r"([^/@]+)/([^/@]+)@([^/@]+)").unwrap();
                match re.captures(&name.resolve()) {
                    Some(c) => {
                        let (user, name, version) = (
                            c.get(1).unwrap().as_str(),
                            c.get(2).unwrap().as_str(),
                            c.get(3).unwrap().as_str()
                        );
                        module_path.push(user);
                        module_path.push(name);
                        module_path.push(version);
                        // now std modules will be fetched from github
                        (format!("https://github.com/{}/{}", user, name), Version::Tag(version.to_string()))
                    },
                    None => return escalate!(Box::new(module::ModuleError{
                        cause: format!("{} is not valid standard module path", name)
                    })),
                }
            },
            module::Source::Git{git, rev, tag} => {
                let url = url::Url::parse(&git.resolve())?;
                module_path.push(match url.host_str() {
                    Some(h) => h,
                    None => return escalate!(Box::new(module::ModuleError{
                        cause: format!("git url should have hostname: {}", git)
                    }))
                });
                match url.path_segments() {
                    Some(ss) => for c in ss {
                        module_path.push(c);
                    },
                    None => {}
                }
                (git.to_string(), match rev {
                    Some(r) => {
                        module_path.push(r.resolve());
                        Version::Hash(r.to_string())
                    },
                    None => match tag {
                        Some(t) => {
                            module_path.push(t.resolve());
                            Version::Tag(t.to_string())
                        },
                        None => {
                            return escalate!(Box::new(module::ModuleError{
                                cause: "when module is fetched from git repository, either revision or tag should be set to get stable build result".to_string()
                            }))
                        }
                    }
                })
            },
            module::Source::Package{url} => {
                return escalate!(Box::new(module::ModuleError{
                    cause: format!("donwload tarball from {} does not support", url)
                }))
            },
            module::Source::Local{path} => {
                let mut p = PathBuf::from(&path.resolve());
                p.push(module::repos::DEPLO_MODULE_CONFIG_FILE_NAME);
                return module::Module::with(&p.to_string_lossy().to_string());
            }
        };
        let shell_opts = if config.runtime.verbosity > 2 {
            shell::no_capture()
        } else {
            shell::capture()
        };
        match fs::metadata(&module_path) {
            Ok(mata) => if mata.is_dir() {
                log::debug!("module already exists at {:?}", module_path);
            } else {
                panic!("module should already exists at {:?} but its not directory", module_path);
            },
            Err(_) => match ver {
                Version::Hash(h) => {
                    shell.exec(shell::args![
                        "git", "clone", 
                        "-c", format!("remote.origin.fetch=+{hash}:refs/remotes/origin/{hash}", hash = h),
                        "--depth", "1", url, module_path.to_str().expect("module path should be valid path")
                    ], shell::no_env(), shell::no_cwd(), &shell_opts)?;
                },
                Version::Tag(t) => {
                    shell.exec(shell::args![
                        "git", "clone", "--depth", "1", "--branch", t, url, 
                        module_path.to_str().expect("module path should be valid path")
                    ], shell::no_env(), shell::no_cwd(), &shell_opts)?;
                }
            }
        };
        module_path.push(DEPLO_MODULE_CONFIG_FILE_NAME);
        module::Module::with(&module_path.to_string_lossy().to_string())
    }
}