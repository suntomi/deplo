use std::error::Error;
use std::result::Result;

use clap::{App, Arg, ArgMatches};

use crate::args;
use crate::cli;

pub struct Clap<'a> {
    pub matches: &'a ArgMatches
}
    
lazy_static! {
    static ref G_ROOT_MATCH: ArgMatches = App::new("deplo")
        .version(cli::version())
        .author("umegaya <iyatomi@gmail.com>")
        .about("deploy everything for mobile game")
        .arg(Arg::with_name("config")
            .short('c')
            .long("config")
            .value_name("FILE")
            .help("Sets a custom config file")
            .takes_value(true))
        .arg(Arg::with_name("dryrun")
            .long("dryrun")
            .help("Prints executed commands instead of invoking them")
            .takes_value(false))
        .arg(Arg::with_name("debug")
            .short('d')
            .long("debug")
            .multiple(true)
            .value_name("CATEGORY")
            .help("Activate debug feature (vcs:deploy:tf:ci)")
            .takes_value(true))
        .arg(Arg::with_name("verbosity")
            .short('v')
            .long("verbose")
            .multiple(true)
            .help("Sets the level of verbosity")
            .takes_value(false))
        .subcommand(
            App::new("init")
                .about("initialize deplo project. need to configure deplo.json beforehand"))
        .subcommand(
            App::new("exec")
                .about("wrap 3rdparty command to dryrun")
                .arg(Arg::new("args")
                    .multiple(true)
                    .help("command name and arguments")
                    .index(1)
                    .required(true)))
        .get_matches();
}

impl<'a> args::Args for Clap<'a> {
    fn create() -> Result<Clap<'a>, Box<dyn Error>> {
        return Ok(Clap::<'a> {
            matches: &G_ROOT_MATCH
        })
    }
    fn subcommand(&self) -> Option<(&str, Self)> {
        match self.matches.subcommand_name() {
            Some(name) => {
                match self.matches.subcommand_matches(name) {
                    Some(m) => Some((name, Clap::<'a>{matches: m})),
                    None => None
                }
            },
            None => None
        }
    }
    fn values_of(&self, name: &str) -> Option<Vec<&str>> {
        match self.matches.values_of(name) {
            Some(it) => Some(it.collect()),
            None => None
        }
    }
}
