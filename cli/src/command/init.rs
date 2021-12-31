use std::error::Error;
use std::fs;

use log;

use core::args;
use core::config;
use core::shell;
use core::util::rm;

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
        config::Config::prepare_ci(&self.config, reinit == "all" || reinit == "ci")?;
        config::Config::prepare_vcs(&self.config, reinit == "all" || reinit == "vcs")?;
        let config = self.config.borrow();
        let mut data_path = config.deplo_data_path()?;
        data_path.push("..");
        data_path.push("deplow");
        rm(&data_path);
        fs::write(&data_path, config.generate_wrapper_script())?;
        self.shell.exec(&vec!["chmod", "+x", data_path.to_str().unwrap()], shell::no_env(), shell::no_cwd(), false)?;
        return Ok(())
    }
}
