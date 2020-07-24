use std::error::Error;
use std::fs;

use log;
use fs_extra;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;
use crate::endpoints;
use crate::tf;

pub struct Init<'a, S: shell::Shell<'a> = shell::Default<'a>> {
    pub config: &'a config::Config,
    pub shell: S
}

impl<'a, S: shell::Shell<'a>, A: args::Args> command::Command<'a, A> for Init<'a, S> {
    fn new(config: &'a config::Config) -> Result<Init<'a, S>, Box<dyn Error>> {
        return Ok(Init::<'a, S> {
            config: config,
            shell: S::new(config)
        });
    }
    fn run(&self, args: &A) -> Result<(), Box<dyn Error>> {
        log::info!("init command invoked");
        fs::create_dir_all(&self.config.root_path())?;
        fs::create_dir_all(&self.config.services_path())?;
        let cloud_provider_and_configs = self.config.cloud_provider_and_configs();
        if args.occurence_of("reinit") > 0 {
            for (provider_code, _) in &cloud_provider_and_configs {
                match fs::remove_dir_all(self.config.infra_code_dest_path(provider_code)) {
                    Ok(_) => {},
                    Err(err) => { 
                        log::error!(
                            "fail to cleanup for reinitialize: fail to remove {} with {:?}",
                            self.config.infra_code_dest_path(provider_code).to_string_lossy(), err
                        )
                    },
                }
            }
            for (name, _) in &self.config.lb {
                for (k, _) in &self.config.common.release_targets {
                    match fs::remove_file(self.config.endpoints_file_path(name, Some(k))) {
                        Ok(_) => {},
                        Err(err) => { 
                            log::error!(
                                "fail to cleanup for reinitialize: fail to remove {} with {:?}",
                                self.config.endpoints_file_path(name, Some(k)).to_string_lossy(), err
                            )
                        },    
                    }
                }
            }
        }
        for (provider_code, _) in &cloud_provider_and_configs {
            match fs::metadata(self.config.infra_code_dest_path(provider_code)) {
                Ok(_) => log::info!("infra setup scripts for {} already copied", provider_code),
                Err(_) => {
                    log::debug!("copy infra setup scripts: {}=>{}", 
                        self.config.infra_code_source_path(provider_code).to_str().unwrap(), 
                        self.config.infra_code_dest_path(provider_code).to_str().unwrap()
                    );
                    fs_extra::dir::copy(
                        self.config.infra_code_source_path(provider_code),
                        self.config.infra_code_dest_path(provider_code),
                        &fs_extra::dir::CopyOptions{
                            overwrite: true,
                            skip_exist: false,
                            buffer_size: 64 * 1024, //64kb
                            copy_inside: true,
                            depth: 0
                        }
                    )?;
                }
            }
        }
        log::debug!("create new environment by terraformer");
        let tf = tf::factory(self.config)?;
        let c = self.config.cloud_service("default")?;
        tf.init(&c)?;
        tf.exec()?;

        log::debug!("create CI setting");
        /* for (account_name, ci) in &self.config.ci_caches {
            ci.init()?;
        } */

        log::debug!("create endpoints files for each release target");
        fs::create_dir_all(&self.config.endpoints_path())?;
        let root_domain = self.config.root_domain()?;
        for (lb_name, _) in &self.config.lb {
            for (k, _) in &self.config.common.release_targets {
                match fs::metadata(self.config.endpoints_file_path(lb_name, Some(k))) {
                    Ok(_) => log::info!("versions file for [{}] already created", k),
                    Err(_) => {
                        log::info!("create versions file for [{}]", k);
                        let domain = if lb_name == "default" {
                            format!("{}.{}", k, root_domain)
                        } else {
                            format!("{}.{}.{}", k, lb_name, root_domain)
                        };
                        let ep = endpoints::Endpoints::new(lb_name, &domain);
                        ep.save(self.config.endpoints_file_path(lb_name, Some(k)))?;
                    }
                }
            }
        }
        return Ok(())
    }
}
