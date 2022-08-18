use std::error::Error;
use std::fs;
use std::path::{PathBuf};
use std::result::Result;

use crate::config;
use crate::secret;
use crate::util::{escalate};

pub struct File {
    pub path: String,
    pub val: String,
}

impl secret::Factory for File {
    fn new(
        name: &str,
        runtime_config: &config::runtime::Config,
        secret_config: &config::secret::Secret
    ) -> Result<Self, Box<dyn Error>> {
        let cwd = match runtime_config.workdir.as_ref() {
            Some(wd) => PathBuf::from(wd),
            None => std::env::current_dir()?
        };
        return Ok(match secret_config {
            config::secret::Secret::File { path } => Self{
                path: path.clone(), val: if config::Config::is_running_on_ci() {
                    match std::env::var(&name) {
                        Ok(val) => val,
                        Err(_) => return escalate!(Box::new(secret::SecretError{
                            cause: format!("env var {} not found", name)
                        }))
                    }
                } else {
                    match fs::read_to_string(cwd.join(&path)) {
                        Ok(val) => val,
                        Err(e) => return escalate!(Box::new(secret::SecretError{
                            cause: format!("file load error {:?} path={}", e, cwd.join(&path).display())
                        }))
                    }
                }
            },
            _ => panic!("unexpected secret type")
        });
    }
}
impl secret::Accessor for File {
    fn var(&self) -> Result<Option<String>, Box<dyn Error>> {
        Ok(Some(self.val.clone()))
    }
}
impl secret::Secret for File {
}
