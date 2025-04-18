use std::error::Error;
use std::fs;
use std::path::{PathBuf};
use std::result::Result;

use crate::config;
use crate::var;
use crate::util::{escalate};

pub struct File {
    pub val: String,
}

impl var::Factory for File {
    fn new(
        name: &str,
        runtime_config: &config::runtime::Config,
        var: &var::Var
    ) -> Result<Self, Box<dyn Error>> {
        let cwd = match runtime_config.workdir.as_ref() {
            Some(wd) => PathBuf::from(wd),
            None => std::env::current_dir()?
        };
        return Ok(match var {
            var::Var::File { path } => Self{
                val: if config::Config::is_running_on_ci() {
                    match std::env::var(&name) {
                        Ok(val) => val,
                        Err(_) => return escalate!(Box::new(var::VarError{
                            cause: format!("env var {} not found", name)
                        }))
                    }
                } else {
                    match fs::read_to_string(cwd.join(&path)) {
                        Ok(val) => val,
                        Err(e) => return escalate!(Box::new(var::VarError{
                            cause: format!("file load error {:?} path={}", e, cwd.join(&path).display())
                        }))
                    }
                }
            },
            _ => panic!("unexpected secret type")
        });
    }
}
impl var::Accessor for File {
    fn var(&self) -> Result<Option<String>, Box<dyn Error>> {
        Ok(Some(self.val.clone()))
    }
}
impl var::Trait for File {
}
