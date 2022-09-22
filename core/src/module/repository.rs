use std::fs;

use maplit::hashmap;
use regex::Regex;

use crate::config;
use crate::module;
use crate::shell;

pub struct Name<'a> {
    pub path: &'a str
}
impl<'a> Path {
    pub fn new(path: &'a str) -> Self {
        Self { path }
    }
    pub fn parse(&self) -> (&'a str, &'a str, &'a str, &'a str, &'a str) {
        let re = Regex::new(r"^(.*)/([^/]+)/([^/]+)([#@])(.+)$")
        match re.capture(self.path) {
            Some(c) => (
                c.get(1).unwrap().as_str(),
                c.get(2).unwrap().as_str(),
                c.get(3).unwrap().as_str(),
                c.get(4).unwrap().as_str(),
                c.get(5).unwrap().as_str()
            ),
            None = panic!("module path = '{}' is invalid format. expect ${{git repo url}}@${{version}} but {}", self.name)
        }
    }
}

pub struct Repository<S: shell::Shell = shell::Default> {
    instances: HashMap<String, module::config::Config>,
    shell: S
}
impl<S: shell::Shell> Repository<S> {
    pub fn new(config: &config::Config) -> Self {
        Self {
            instances: hashmap!{},
            shell: S::new(config)
        }
    }
    pub fn load(&'a self, config: &config::Config, path: &str) -> &'a module::config::Config {
        self.instances.entry(path.to_string()).or_insert(self.fetch(path))
    }
    pub fn fetch(&self, path: &str) -> module::config::Config {
        let (url_prefix, user, name, specifier, version) = Path::new(path).parse();
        if url_prefix.len() <= 0 {
            panic!("standard module repository have not created yet {}", path);
        } else {
            
        }
    }
}