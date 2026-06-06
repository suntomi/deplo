use std::collections::HashMap;
use std::error::Error;

use core::config;
use core::shell;
use core::util::json_to_strmap;

use crate::args;
use crate::command;
use crate::util::escalate;

pub struct VCS<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    #[allow(dead_code)]
    pub shell: S
}
impl<S: shell::Shell> VCS<S> {
    fn release<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let vcs = config.modules.vcs();
        vcs.release(
            (args.value_or_die("tag_name"), false),
            &args.json_value_of("option")?
        )?;
        Ok(())
    }
    fn release_assets<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let vcs = config.modules.vcs();
        let mut options = args.json_value_of("option")?;
        if args.get_flag("replace") {
            options.as_object_mut().unwrap().insert("replace".to_string(), serde_json::json!(true));
        }
        vcs.release_assets(
            (args.value_or_die("tag_name"), false),
            args.value_or_die("asset_file_path"),
            &options
        )?;
        Ok(())
    }
    fn control_pr<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let vcs = config.modules.vcs();
        match args.subcommand() {
            Some(("create", subargs)) => {
                let option_values = subargs.json_value_of("option")?;
                let option_map = json_to_strmap(&option_values);
                let title = match option_map.get("title") {
                    Some(v) => v,
                    None => return escalate!(subargs.error("pr create: must specify -o title=$title"))
                };
                let head = match option_map.get("head") {
                    Some(v) => v,
                    None => return escalate!(subargs.error("pr create: must specify -o head=$head_branch"))
                };
                let base = match option_map.get("base") {
                    Some(v) => v,
                    None => return escalate!(subargs.error("pr create: must specify -o base=$base_branch"))
                };
                let options = option_map.iter()
                    .filter(|(k, _)| **k != "title" && **k != "head" && **k != "base")
                    .map(|(k, v)| (*k, v.as_str()))
                    .collect::<HashMap<_, _>>();
                vcs.pr(title, head, base, &options)?;
            },
            Some(("search", subargs)) => {
                let filters = subargs.values_of("filter")
                    .map(|v| v.into_iter().map(|s| s.to_string()).collect::<Vec<_>>())
                    .unwrap_or(vec![]);
                println!("{}", vcs.search_pr(&filters)?);
            },
            Some(("merge", subargs)) => {
                vcs.merge_pr(
                    subargs.value_or_die("url"),
                    &subargs.json_value_of("option")?
                )?;
            },
            Some(("close", subargs)) => {
                vcs.close_pr(
                    subargs.value_or_die("url"),
                    &subargs.json_value_of("option")?
                )?;
            },
            _ => return escalate!(args.error("no such subcommand for pr control"))
        }
        Ok(())
    }
    fn control_label<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let vcs = config.modules.vcs();
        match args.subcommand() {
            Some(("create", subargs)) => {
                vcs.label(
                    subargs.value_or_die("label_name"),
                    subargs.value_of("color")
                )?;
            },
            _ => return escalate!(args.error("no such subcommand for label control"))
        }
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
            Some(("pr", subargs)) => return self.control_pr(&subargs),
            Some(("label", subargs)) => return self.control_label(&subargs),
            Some((name, _)) => return escalate!(args.error(
                &format!("no such subcommand: [{}]", name) 
            )),
            None => return escalate!(args.error("no subcommand specified"))
        }
    }
}
