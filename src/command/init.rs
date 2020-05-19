use std::error::Error;
use std::fs;

use log;
use fs_extra;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;
use crate::endpoints;

pub struct Init<'a, S: shell::Shell<'a> = shell::Default<'a>> {
    pub config: &'a config::Config<'a>,
    pub shell: S
}

impl<'a, S: shell::Shell<'a>, A: args::Args> command::Command<'a, A> for Init<'a, S> {
    fn new(config: &'a config::Config) -> Result<Init<'a, S>, Box<dyn Error>> {
        return Ok(Init::<'a, S> {
            config: config,
            shell: S::new(config)
        });
    }
    fn run(&self, _: &A) -> Result<(), Box<dyn Error>> {
        log::info!("init command invoked");
        fs::create_dir_all(&self.config.root_path())?;
        fs::create_dir_all(&self.config.services_path())?;
        log::debug!("copy infra setup scripts: {}=>{}", 
            self.config.infra_code_source_path().to_str().unwrap(), 
            self.config.infra_code_dest_path().to_str().unwrap()
        );
        fs_extra::dir::copy(
            self.config.infra_code_source_path(),
            self.config.infra_code_dest_path(),
            &fs_extra::dir::CopyOptions{
                overwrite: true,
                skip_exist: false,
                buffer_size: 64000, //64kb
                copy_inside: true,
                depth: 0
            }
        )?;
        log::info!("init command invoked3");
        fs::create_dir_all(&self.config.endpoints_path())?;
        for (k, _) in &self.config.common.release_targets {
            log::info!("create versions file for {}", k);
            let ep = endpoints::Endpoints::new(&format!("{}.{}", k, self.config.root_domain()));
            ep.save(self.config.endpoints_file_path(Some(k)))?;
        }
        return Ok(())
    }
}
