use std::error::Error;
use std::result::Result;

use clap::{App, Arg, ArgMatches};

use crate::args;
use crate::cli;

pub struct Clap<'a> {
    pub hierarchy: Vec<&'a str>,
    pub matches: &'a ArgMatches
}
    
lazy_static! {
    static ref G_ROOT_MATCH: ArgMatches = App::new("deplo")
        .version(cli::version())
        .author("umegaya <iyatomi@gmail.com>")
        .about("deploy everything for mobile game")
        .arg(Arg::with_name("dotenv")
            .short('e')
            .long("dotenv")
            .value_name(".ENV FILE")
            .help("specify .env file path")
            .takes_value(true))
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
        .arg(Arg::with_name("workdir")
            .short('w')
            .long("workdir")
            .help("Sets working directory of entire process")
            .takes_value(true))
        .subcommand(
            App::new("init")
                .about("initialize deplo project. need to configure deplo.json beforehand")
                .arg(Arg::new("reinit")
                    .long("reinit")
                    .help("initialize again")
                    .required(true))
        )
        .subcommand(
            App::new("destroy")
                .about("destroy deplo project")
        )
        .subcommand(
            App::new("infra")
                .about("control infrastrucure for deplo")
                .subcommand(
                    App::new("plan")
                    .about("plan infra change")
                )
                .subcommand(
                    App::new("apply")
                    .about("apply infra change")
                )
                .subcommand(
                    App::new("rsc")
                    .about("get resource value or list")
                    .arg(Arg::new("path")
                        .help("resource path. if omitted, list of resource paths are returned")
                        .index(1))
                )
        )
        .subcommand(
            App::new("exec")
                .about("wrap 3rdparty command to dryrun")
                .arg(Arg::new("args")
                    .multiple(true)
                    .help("command name and arguments")
                    .required(true))
        )
        .subcommand(
            App::new("service")
                .about("control service, which represent single deployment unit")
                .subcommand(
                    App::new("create")
                    .about("create service")
                    .arg(Arg::new("name")
                        .help("service name")
                        .index(1)
                        .required(true))
                    .arg(Arg::with_name("type")
                        .short('t')
                        .long("type")
                        .help("specify service type")
                        .possible_values(
                            &["container", "storage", "script"]
                        )
                        .required(true))
                )
                .subcommand(
                    App::new("deploy")
                    .about("deploy service")
                    .arg(Arg::new("name")
                        .help("service name")
                        .index(1)
                        .required(true))
                )    
                .subcommand(
                    App::new("cutover")
                    .about("direct traffic to new version. \
                        use after services updated with deplo service deploy")
                )    
                .subcommand(
                    App::new("cleanup")
                    .about("cleanup unused service deployment. \
                        use after services updated with deplo service cutover")
                )    
        )
        .get_matches();
}

impl<'a> args::Args for Clap<'a> {
    fn create() -> Result<Clap<'a>, Box<dyn Error>> {
        return Ok(Clap::<'a> {
            hierarchy: vec!{},
            matches: &G_ROOT_MATCH
        })
    }
    fn subcommand(&self) -> Option<(&str, Self)> {
        match self.matches.subcommand_name() {
            Some(name) => {
                match self.matches.subcommand_matches(name) {
                    Some(m) => {
                        let mut h = self.hierarchy.clone();
                        h.push(name);
                        Some((name, Clap::<'a>{
                            hierarchy: h,
                            matches: m
                        }))
                    },
                    None => None
                }
            },
            None => None
        }
    }
    fn occurence_of(&self, name: &str) -> u64 {
        return self.matches.occurrences_of(name);
    }
    fn values_of(&self, name: &str) -> Option<Vec<&str>> {
        match self.matches.values_of(name) {
            Some(it) => Some(it.collect()),
            None => None
        }
    }
    fn command_path(&self) -> &Vec<&str> {
        &self.hierarchy
    }
}