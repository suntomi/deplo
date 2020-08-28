use std::error::Error;
use std::fs;

use log;
use fs_extra;

use crate::args;
use crate::config;
use crate::command;
use crate::shell;
use crate::endpoints;

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

        log::info!("create new environment by terraformer");
        let tf = config.terraformer()?;
        tf.init()?;
        tf.exec()?;

        log::info!("create endpoints files for each release target");
        let root_domain = config.root_domain()?;
        for (lb_name, _) in &config.lb {
            for (k, _) in &config.common.release_targets {
                match fs::metadata(config.endpoints_file_path(lb_name, Some(k))) {
                    Ok(_) => if lb_name == "default" {
                        log::info!("versions file for [{}] already created", k)
                    } else {
                        log::info!("versions file for [{}.{}] already created", lb_name, k)
                    },
                    Err(_) => {
                        let domain = if lb_name == "default" {
                            log::info!("create versions file for [{}]", k);
                            fs::create_dir_all(&config.endpoints_path())?;
                            format!("{}.{}", k, root_domain)
                        } else {
                            log::info!("create versions file for [{}.{}]", lb_name, k);
                            fs::create_dir_all(&config.endpoints_path().join(lb_name))?;
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
