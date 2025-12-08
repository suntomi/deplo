use std::error::Error;

use log;

use core::args;
use core::config;
use core::shell;
use core::util::{rm, path_join};

use crate::command;

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
        log::debug!("init command invoked");
        // do preparation
        let reinit = args.value_of("reinit").unwrap_or("none");
        let config = self.config.borrow();
        let data_path = path_join(vec![config.deplo_data_path()?.to_str().unwrap(), "..", "deplow"]);
        rm(&data_path);
        config.generate_wrapper_script(&self.shell, &data_path)?;
        for (k, v) in config.modules.ci() {
            log::debug!("generating ci config for account [{}]", k);
            v.generate_config(reinit == "all" || reinit == "ci")?;
        }
        return Ok(())
    }
}
