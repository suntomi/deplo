use std::error::Error;
use std::result::Result;

use clap::{App, Arg, ArgMatches};

use crate::args;
use crate::config;

pub struct Clap<'a> {
    pub hierarchy: Vec<&'a str>,
    pub matches: &'a ArgMatches
}
    
lazy_static! {
    static ref G_ROOT_MATCH: ArgMatches = App::new("deplo")
        .version(config::DEPLO_VERSION)
        .author("umegaya <iyatomi@gmail.com>")
        .about("write once, run anywhere for CI/CD")
        .arg(Arg::new("config")
            .short('c')
            .long("config")
            .value_name("FILE")
            .about("Sets a custom config file")
            .takes_value(true))
        .arg(Arg::new("debug")
            .short('d')
            .long("debug")
            .value_name("KEY(=VALUE),...")
            .about("Activate debug feature\n\
                possible settings(concat with comma when specify multiple values): \n\
                skip_set_secret=flag)\n\
                force_set_release_target_to=(one of your release target)\n\
            ")
            .takes_value(true))
        .arg(Arg::new("dotenv")
            .short('e')
            .long("dotenv")
            .value_name(".ENV FILE OR TEXT")
            .about("specify .env file path or .env file content directly")
            .takes_value(true))
        .arg(Arg::new("dryrun")
            .long("dryrun")
            .about("Prints executed commands instead of invoking them")
            .takes_value(false))
        .arg(Arg::new("verbosity")
            .short('v')
            .long("verbose")
            .multiple_values(true)
            .about("Sets the level of verbosity")
            .takes_value(true))
        .arg(Arg::new("workdir")
            .short('w')
            .long("workdir")
            .about("Sets working directory of entire process")
            .takes_value(true))
        .subcommand(
            App::new("info")
                .about("get information about deplo")
                .subcommand(
                    App::new("version")
                        .about("get deplo version")
                        .arg(Arg::new("output")
                            .about("output format")
                            .short('o')
                            .long("output")
                            .possible_values(
                                &["plain", "json"]
                            )
                            .required(false))
                )
        )
        .subcommand(
            App::new("init")
                .about("initialize deplo project. need to configure deplo.json beforehand")
                .arg(Arg::new("reinit")
                    .long("reinit")
                    .about("initialize component")
                    .required(false)
                    .takes_value(true)
                    .possible_values(
                        &["ci", "vcs", "all"]
                    ))
        )
        .subcommand(
            App::new("destroy")
                .about("destroy deplo project")
        )
        .subcommand(
            App::new("ci")
                .about("handling CI input/control CI settings")
                .subcommand(
                    App::new("kick")
                    .about("entry point of CI/CD process")
                )
                .subcommand(
                    App::new("run")
                    .about("run specific job in Deplo.toml. used for auto generated CI/CD settings")
                    .arg(Arg::new("name")
                        .about("job name")
                        .index(1)
                        .required(true))
                )
                .subcommand(
                    App::new("setenv")
                    .about("upload current .env contents as CI service secrets")
                )
                .subcommand(
                    App::new("fin")
                    .about("cleanup CI/CD process after all related job finished")
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