use std::collections::{HashMap};
use std::error::Error;
use std::result::Result;

use clap::{App, Arg, ArgMatches};
use maplit::hashmap;

use crate::args;
use crate::config;

pub struct Clap<'a> {
    pub hierarchy: Vec<&'a str>,
    pub matches: &'a ArgMatches,
    pub ailiased_path: Vec<&'a str>,
}

struct CommandFactory {
    factory: fn (&'static str) -> App<'static>,
}
impl CommandFactory {
    fn new(f: fn (&'static str) -> App<'static>) -> Self {
        Self { factory: f }
    }
    fn create(&self, name: &'static str) -> App<'static> {
        (self.factory)(name)
    }
}
struct AliasCommands {
    map: HashMap<&'static str, (Vec<&'static str>, CommandFactory)>,
    alias_map: HashMap<&'static str, &'static str>
}
impl AliasCommands {
    fn new(
        map: HashMap<&'static str, (Vec<&'static str>, CommandFactory)>,
        alias_map: HashMap<&'static str, &'static str>
    ) -> Self {
        Self { map, alias_map }
    }
    fn create(&self, name: &'static str) -> App<'static> {
        match self.alias_map.get(name) {
            Some(body) => {
                self.map.get(body).unwrap().1.create(name)
            },
            None => match self.map.get(name) {
                Some(ent) => ent.1.create(ent.0.last().unwrap()),
                None => panic!("no such aliased command [{}]", name)
            }
        }
    }
    fn find_path(&self, name: &str) -> Vec<&'static str> {
        match self.alias_map.get(name) {
            Some(body) => self.map.get(body).unwrap().0.clone(),
            None => vec![]
        }
    }
}

fn job_running_command_options(
    name: &'static str,
    help: &'static str
) -> App<'static> {
    App::new(name)
    .override_help(help)
    .arg(Arg::new("name")
        .help("job name")
        .index(1)
        .required(true))
    .arg(Arg::new("env")
        .help("only works with --remote, set adhoc environment variables for remote job. \n\
               can specify multiple times")
        .long("env")
        .short('e')
        .required(false))
    .arg(Arg::new("async")
        .help("only works with --remote, don't wait for finishing remote job")
        .long("async")
        .required(false))
    .arg(Arg::new("remote")
        .help("if set, run the job on the correspond remote CI service")
        .long("remote")
        .required(false))
    .arg(Arg::new("ref")
        .help("git ref to run the job")
        .long("ref")
        .takes_value(true)
        .required(false))
    .subcommand(
        App::new("sh")
            .override_help("running arbiter command for environment of jobs")
            .arg(Arg::new("task")
                .help("running command that declared in tasks directive")
                .multiple_values(true)))    
}

lazy_static! {
    static ref G_ALIASED_COMMANDS: AliasCommands = AliasCommands::new(hashmap!{
        "ci_deploy" => (
            vec!["ci", "deploy"], CommandFactory::new(|name| -> App<'static> { 
                job_running_command_options(
                    name, 
                    "run specific deploy job in Deplo.toml. used for auto generated CI/CD settings"
                )
            })
        ),
        "ci_integrate" => (
            vec!["ci", "integrate"], CommandFactory::new(|name| -> App<'static> { 
                job_running_command_options(
                    name, 
                    "run specific integrate job in Deplo.toml. used for auto generated CI/CD settings"
                )
            })
        ),
    }, hashmap!{
        // define aliased path like
        // "$subcommand/$subcommand_of_subcommand/$subcommand_of_subcommand_of_subcommand/..." => 
        // ["$other_subcommand", "$subcommand_of_other_subcommand", ...]
        // you also add matches for both corresponding subcommand path of G_ROOT_MATCH.
        "d" => "ci_deploy",    // linked to G_ALIASED_COMMANDS.map["ci_deploy"]
        "i" => "ci_integrate", // linked to G_ALIASED_COMMANDS.map["ci_integrate"]
    });
    static ref G_ROOT_MATCH: ArgMatches = App::new("deplo")
        .version(config::DEPLO_VERSION)
        .author("umegaya <iyatomi@gmail.com>")
        .override_help("write once, run anywhere for CI/CD")
        .arg(Arg::new("config")
            .short('c')
            .long("config")
            .value_name("FILE")
            .help("Sets a custom config file")
            .takes_value(true))
        .arg(Arg::new("release-target")
            .help("force set release target")
            .short('r')
            .long("release-target")
            .takes_value(true))
        .arg(Arg::new("debug")
            .short('d')
            .long("debug")
            .value_name("KEY(=VALUE),...")
            .help("Activate debug feature\n\
                    TODO: add some debug flags")
            .takes_value(true)
            .multiple_values(true)
            .multiple_occurrences(true))
        .arg(Arg::new("dotenv")
            .short('e')
            .long("dotenv")
            .value_name(".ENV FILE OR TEXT")
            .help("specify .env file path or .env file content directly")
            .takes_value(true))
        .arg(Arg::new("dryrun")
            .long("dryrun")
            .help("Prints executed commands instead of invoking them")
            .takes_value(false))
        .arg(Arg::new("verbosity")
            .short('v')
            .long("verbose")
            .help("Sets the level of verbosity")
            .takes_value(true))
        .arg(Arg::new("workflow-type")
            .long("workflow")
            .short('w')
            .help("Sets workflow type of current run")
            .takes_value(true))
        .arg(Arg::new("workdir")
            .long("workdir")
            .help("Sets workflow type of current run")
            .takes_value(true))
        .subcommand(
            App::new("info")
                .override_help("get information about deplo")
                .subcommand(
                    App::new("version")
                        .override_help("get deplo version")
                        .arg(Arg::new("output")
                            .help("output format")
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
                .override_help("initialize deplo project. need to configure deplo.json beforehand")
                .arg(Arg::new("reinit")
                    .long("reinit")
                    .help("initialize component")
                    .required(false)
                    .takes_value(true)
                    .possible_values(
                        &["ci", "vcs", "all"]
                    ))
        )
        .subcommand(
            App::new("destroy")
                .override_help("destroy deplo project")
        )
        .subcommand(
            G_ALIASED_COMMANDS.create("d")
        )
        .subcommand(
            G_ALIASED_COMMANDS.create("i")
        )
        .subcommand(
            App::new("ci")
                .override_help("handling CI input/control CI settings")
                .subcommand(
                    App::new("kick")
                    .override_help("entry point of CI/CD process")
                )
                .subcommand(
                    G_ALIASED_COMMANDS.create("ci_deploy")
                )
                .subcommand(
                    G_ALIASED_COMMANDS.create("ci_integrate")
                )
                .subcommand(
                    App::new("setenv")
                    .override_help("upload current .env contents as CI service secrets")
                )
                .subcommand(
                    App::new("fin")
                    .override_help("cleanup CI/CD process after all related job finished")
                )
        )
        .subcommand(
            App::new("vcs")
                .override_help("control VCS resources")
                .subcommand(
                    App::new("release")
                    .override_help("create release")
                    .arg(Arg::new("tag_name")
                        .help("tag name to use for release")
                        .index(1)
                        .required(true))
                    .arg(Arg::new("option")
                        .help("option for release creation.\n\
                                -o $key=$value\n\
                                for github, body options of https://docs.github.com/en/rest/reference/releases#create-a-release can be specified.\n\
                                TODO: for gitlab")
                        .short('o')
                        .takes_value(true)
                        .multiple_values(true)
                        .multiple_occurrences(true))
                )
                .subcommand(
                    App::new("release-assets")
                    .override_help("upload release assets")
                    .arg(Arg::new("tag_name")
                        .help("tag name to use for release")
                        .index(1)
                        .required(true))
                    .arg(Arg::new("asset_file_path")
                        .help("file path for upload file")
                        .index(2)
                        .required(true))
                    .arg(Arg::new("replace")
                        .help("replace existing asset or not")
                        .long("replace"))
                    .arg(Arg::new("option")
                        .help("option for release creation.\n\
                                -o name=$release_asset_name\n\
                                -o content-type=$content_type_of_asset\n\
                                TODO: implement more options")
                        .short('o')
                        .takes_value(true)
                        .multiple_values(true)
                        .multiple_occurrences(true))
                )
        )        
        .get_matches();
}
impl<'a> Clap<'a> {
    fn find_aliased_path(&self, name: &str) -> Vec<&'a str> {
        let mut h = self.hierarchy.clone();
        h.push(name);
        let key = h.join("/");
        G_ALIASED_COMMANDS.find_path(&key)
    }
}

impl<'a> args::Args for Clap<'a> {
    fn create() -> Result<Clap<'a>, Box<dyn Error>> {
        return Ok(Clap::<'a> {
            hierarchy: vec!{},
            matches: &G_ROOT_MATCH,
            ailiased_path: vec!{}
        })
    }
    fn subcommand(&self) -> Option<(&str, Self)> {
        let (may_subcommand_name, aliased) = if self.ailiased_path.len() > 0 {
            (Some(self.ailiased_path[0]), true)
        } else {
            (self.matches.subcommand_name(), false)
        };
        match may_subcommand_name {
            Some(name) => {
                if aliased {
                    let mut h = self.hierarchy.clone();
                    let mut ap = self.ailiased_path.clone();
                    h.push(name);
                    ap.pop();
                    Some((name, Clap::<'a>{
                        hierarchy: h,
                        matches: self.matches,
                        ailiased_path: ap
                    }))
                } else {
                    match self.matches.subcommand_matches(name) {
                        Some(m) => {
                            let mut h = self.hierarchy.clone();
                            let mut ap = self.find_aliased_path(name).clone();
                            let may_aliased_name = if ap.len() > 0 {
                                let aliased = ap.remove(0);
                                h.push(aliased);
                                aliased
                            } else {
                                h.push(name);
                                name
                            };
                            Some((may_aliased_name, Clap::<'a>{
                                hierarchy: h,
                                matches: m,
                                ailiased_path: ap
                            }))
                        },
                        None => None
                    }
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