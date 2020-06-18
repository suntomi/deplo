use std::error::Error;

use log;
use maplit::hashmap;
use glob::glob;

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

impl<'a, S: shell::Shell<'a>, A: args::Args> command::Command<'a, A> for CI<'a, S> {
    fn new(config: &'a config::Config) -> Result<CI<'a, S>, Box<dyn Error>> {
        return Ok(CI::<'a, S> {
            config: config,
            ci: config.ci_service()?,
            shell: S::new(config)
        });
    }
    fn run(&self, args: &A) -> Result<(), Box<dyn Error>> {
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
                if self.ci.changed(&patterns.split(',').collect()) {
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
                        let stem = path.file_stem().unwrap().to_string_lossy();
                        log::debug!("plan file path:{},stem:{}", path.to_string_lossy(), stem);
                        if self.ci.changed(&vec!(&format!("{}/.*", stem))) {
                            self.shell.eval(&format!("deplo service action {}", stem), &hashmap!{}, false)?;
                        }                        
                    },
                    Err(e) => return escalate!(Box::new(e))
                }             
            }

        }
        Ok(())
    }
}
