use std::error::Error;

use core::config;
use core::shell;

use crate::args;
use crate::command;
use crate::util::escalate;

pub struct VCS<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub shell: S
}
impl<S: shell::Shell> VCS<S> {
    fn release<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let vcs = config.vcs_service()?;
        vcs.release(
            (args.value_of("tag_name").unwrap(), false),
            &args.json_value_of("option")?
        )?;
        Ok(())
    }
    fn release_assets<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let vcs = config.vcs_service()?;
        let mut options = args.json_value_of("option")?;
        if args.occurence_of("replace") > 0 {
            options.as_object_mut().unwrap().insert("replace".to_string(), serde_json::json!(true));
        }
        vcs.release_assets(
            (args.value_of("tag_name").unwrap(), false),
            args.value_of("asset_file_path").unwrap(),
            &options
        )?;
        Ok(())
    }
}

impl<S: shell::Shell, A: args::Args> command::Command<A> for VCS<S> {
    fn new(config: &config::Container) -> Result<VCS<S>, Box<dyn Error>> {
        return Ok(VCS::<S> {
            config: config.clone(),
            shell: S::new(config)
        });
    }
    fn run(&self, args: &A) -> Result<(), Box<dyn Error>> {
        match args.subcommand() {
            Some(("release", subargs)) => return self.release(&subargs),
            Some(("release-assets", subargs)) => return self.release_assets(&subargs),
            Some((name, _)) => return escalate!(args.error(
                &format!("no such subcommand: [{}]", name) 
            )),
            None => return escalate!(args.error("no subcommand specified"))
        }
    }
}
