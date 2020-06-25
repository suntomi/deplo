use std::error::Error;
use std::fs;
use std::io::{BufReader, BufRead};

use log;
use maplit::hashmap;
use glob::glob;
use dotenv::dotenv;
use regex::Regex;

use crate::args;
use crate::ci;
use crate::config;
use crate::command;
use crate::shell;
use crate::util::escalate;

pub struct CI<'a, S: shell::Shell<'a> = shell::Default<'a>> {
    pub config: &'a config::Config<'a>,
    pub ci: Box<dyn ci::CI<'a> + 'a>,
    pub shell: S
}
impl<'a, S: shell::Shell<'a>> CI<'a, S> {
    fn kick<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        log::info!("ci command invoked");
        if !match std::env::var("DEPLO_CI_TYPE") {
            Ok(v) => self.config.ci.type_matched(&v),
            Err(e) => match e {
                std::env::VarError::NotPresent => true,
                _ => return escalate!(Box::new(e))
            }
        } {
            log::info!("ci type does not matched: expect:{} but run on:{}", 
                self.config.ci, std::env::var("DEPLO_CI_TYPE").unwrap()
            );
            return Ok(())
        }
        let config = match self.ci.pull_request_url()? {
            Some(_) => &self.config.action.pr,
            None => &self.config.action.deploy,
        };
        if config.len() > 0 {
            for (patterns, code) in config {
                let ps = &patterns.split(',').map(|p| {
                    std::env::current_dir().unwrap()
                        .join(p)
                        .to_string_lossy().to_string()
                }).collect::<Vec<String>>();
                if self.ci.changed(&ps.iter().map(std::ops::Deref::deref).collect()) {
                    self.shell.run_code_or_file(&code, &hashmap!{})?;
                }
            }
        } else {
            // if no action config and has some plan files, deplo try to find diff files
            // which path is start with the same name of plan file's basename
            // eg. if we have plan file which name is 'foo.toml', deplo check diff with 
            // pattern 'foo/.*' and if diff exists, call 'deplo service adtion foo'
            for entry in glob(&self.config.services_path().join("*.toml").to_string_lossy())? {
                match entry {
                    Ok(path) => {
                        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
                        log::debug!("plan file path:{},stem:{}", path.to_string_lossy(), stem);
                        match std::env::current_dir()?.join(&stem).join(".*").to_str() {
                            Some(p) => if self.ci.changed(&vec!(p)) {
                                self.shell.eval(&format!("deplo service action {}", stem), &hashmap!{}, false)?;
                            },
                            None => {}
                        }
                    },
                    Err(e) => return escalate!(Box::new(e))
                }             
            }

        }
        Ok(())
    }
    fn setenv<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        let dotenv_file_content = match args.value_of("dotenv") {
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
                Err(err) => return escalate!(
                    args.error(&format!("no .env file found err:{:?}", err))
                ),
            }
        };
        let r = BufReader::new(dotenv_file_content.as_bytes());
        let re = Regex::new(r#"^([^=]+)=(.+)$"#).unwrap();
        for read_result in r.lines() {
            match read_result {
                Ok(line) => match re.captures(&line) {
                    Some(c) => {
                        self.ci.set_secret(
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

impl<'a, S: shell::Shell<'a>, A: args::Args> command::Command<'a, A> for CI<'a, S> {
    fn new(config: &'a config::Config) -> Result<CI<'a, S>, Box<dyn Error>> {
        return Ok(CI::<'a, S> {
            config: config,
            ci: config.ci_service()?,
            shell: S::new(config)
        });
    }
    fn run(&self, args: &A) -> Result<(), Box<dyn Error>> {
        match args.subcommand() {
            Some(("kick", subargs)) => return self.kick(&subargs),
            Some(("setenv", subargs)) => return self.setenv(&subargs),
            Some((name, _)) => return escalate!(args.error(
                &format!("no such subcommand: [{}]", name) 
            )),
            None => return escalate!(args.error("no subcommand specified"))
        }
    }
}