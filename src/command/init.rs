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

pub struct Init<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub shell: S
}

impl<S: shell::Shell, A: args::Args> command::Command<A> for Init<S> {
    fn new(config: &config::Container) -> Result<Init<S>, Box<dyn Error>> {
        return Ok(Init::<S> {
            config: config.clone(),
            shell: S::new(config)
        });
    }
    fn run(&self, args: &A) -> Result<(), Box<dyn Error>> {
        log::info!("init command invoked");
        let config = self.config.borrow();
        fs::create_dir_all(&config.root_path())?;
        fs::create_dir_all(&config.services_path())?;
        let cloud_provider_and_configs = config.cloud_provider_and_configs();
        if args.occurence_of("reinit") > 0 {
            for (provider_code, _) in &cloud_provider_and_configs {
                match fs::remove_dir_all(config.infra_code_dest_path(provider_code)) {
                    Ok(_) => {},
                    Err(err) => { 
                        log::error!(
                            "fail to cleanup for reinitialize: fail to remove {} with {:?}",
                            config.infra_code_dest_path(provider_code).to_string_lossy(), err
                        )
                    },
                }
            }
            for (name, _) in &config.lb {
                for (k, _) in &config.common.release_targets {
                    match fs::remove_file(config.endpoints_file_path(name, Some(k))) {
                        Ok(_) => {},
                        Err(err) => { 
                            log::error!(
                                "fail to cleanup for reinitialize: fail to remove {} with {:?}",
                                config.endpoints_file_path(name, Some(k)).to_string_lossy(), err
                            )
                        },    
                    }
                }
            }
        }
        for (provider_code, _) in &cloud_provider_and_configs {
            match fs::metadata(config.infra_code_dest_path(provider_code)) {
                Ok(_) => log::info!("infra setup scripts for {} already copied", provider_code),
                Err(_) => {
                    log::debug!("copy infra setup scripts: {}=>{}", 
                        config.infra_code_source_path(provider_code).to_str().unwrap(), 
                        config.infra_code_dest_path(provider_code).to_str().unwrap()
                    );
                    fs_extra::dir::copy(
                        config.infra_code_source_path(provider_code),
                        config.infra_code_dest_path(provider_code),
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
        let tf = tf::factory(&self.config)?;
        let c = config.cloud_service("default")?;
        tf.init(&c)?;
        tf.exec()?;

        log::debug!("create CI setting");
        /* for (account_name, ci) in &self.config.ci_caches {
            ci.init()?;
        } */

        log::debug!("create endpoints files for each release target");
        fs::create_dir_all(&config.endpoints_path())?;
        let root_domain = config.root_domain()?;
        for (lb_name, _) in &config.lb {
            for (k, _) in &config.common.release_targets {
                match fs::metadata(config.endpoints_file_path(lb_name, Some(k))) {
                    Ok(_) => log::info!("versions file for [{}] already created", k),
                    Err(_) => {
                        log::info!("create versions file for [{}]", k);
                        let domain = if lb_name == "default" {
                            format!("{}.{}", k, root_domain)
                        } else {
                            format!("{}.{}.{}", k, lb_name, root_domain)
                        };
                        let ep = endpoints::Endpoints::new(lb_name, &domain);
                        ep.save(config.endpoints_file_path(lb_name, Some(k)))?;
                    }
                }
            }
        }
        return Ok(())
    }
}
