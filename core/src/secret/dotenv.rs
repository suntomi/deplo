use std::collections::{HashMap};
use std::error::Error;
use std::fs;
use std::io::{BufReader, BufRead};
use std::result::Result;

use dotenv::dotenv;
use maplit::hashmap;
use regex::Regex;

use crate::config;
use crate::secret;
use crate::util::{escalate};

pub struct Dotenv {
    pub path: Option<String>,
    pub secrets: HashMap<String, String>
}

impl Dotenv {
    pub fn parse_dotenv<F>(path: &Option<String>, mut cb: F) -> Result<(), Box<dyn Error>>
    where F: FnMut (&str, &str) -> Result<(), Box<dyn Error>> {
        let dotenv_file_content = match path {
            Some(dotenv_path) => match fs::metadata(dotenv_path) {
                Ok(_) => match fs::read_to_string(dotenv_path) {
                    Ok(content) => content,
                    Err(err) => return escalate!(Box::new(err))
                },
                Err(_) => dotenv_path.to_string(),
            },
            None => match dotenv() {
                Ok(dotenv_path) => match fs::read_to_string(dotenv_path) {
                    Ok(content) => content,
                    Err(err) => return escalate!(Box::new(err))
                },
                Err(err) => return escalate!(Box::new(err))
            }
        };
        let r = BufReader::new(dotenv_file_content.as_bytes());
        let re = Regex::new(r#"^([^=]+)=(.+)$"#).unwrap();
        for read_result in r.lines() {
            match read_result {
                Ok(line) => match re.captures(&line) {
                    Some(c) => {
                        cb(
                            c.get(1).map(|m| m.as_str()).unwrap(),
                            c.get(2).map(|m| m.as_str()).unwrap().trim_matches('"')
                        )?;
                    },
                    None => {},
                },
                Err(_) => {}
            }
        }
        return Ok(())
    }    
}

impl secret::Factory for Dotenv {
    fn new(
        secret_config: &config::secret::Secret
    ) -> Result<Dotenv, Box<dyn Error>> {
        return Ok(match secret_config {
            config::secret::Secret::Dotenv { path, .. } => {
                let mut r = Dotenv{
                    secrets: hashmap!{},
                    path: path.clone()
                };
                Self::parse_dotenv(&r.path, |k,v| {
                    r.secrets.insert(k.to_string(), v.to_string());
                    Ok(())
                })?;
                r
            },
            _ => panic!("unexpected secret type")
        });
    }
}
impl secret::Accessor for Dotenv {
    fn var(&self, key: &str) -> Result<Option<String>, Box<dyn Error>> {
        Ok(self.secrets.get(key).map_or_else(|| None, |v| Some(v.clone())))
    }
    fn vars(&self) -> Result<HashMap<String, String>, Box<dyn Error>> {
        Ok(hashmap!{})
    }
}
impl secret::Secret for Dotenv {
}
