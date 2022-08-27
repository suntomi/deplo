use std::collections::{HashMap};
use std::error::Error;
use std::result::Result;

use clap::{App, Arg, ArgMatches};
use maplit::hashmap;

use crate::args;
use crate::config;

/// represents matched command line arguments by clap.
pub struct Clap<'a> {
    pub hierarchy: Vec<&'a str>,
    pub matches: &'a ArgMatches,
    pub aliased_path: Vec<&'a str>,
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
                self.map.get(body).expect(&format!("alias does not have body for {body}", body = body)).1.create(name)
            },
            None => match self.map.get(name) {
                Some(ent) => ent.1.create(ent.0.last().expect("alias body has no element")),
                None => panic!("no such aliased command [{}]", name)
            }
        }
    }
    fn find_path(&self, name: &str) -> Vec<&'static str> {
        match self.alias_map.get(name) {
            Some(body) => self.map.get(body).expect("alias does not have body").0.clone(),
            None => vec![]
        }
    }
}

fn workflow_command_options(
    name: &'static str,
    about: &'static str,
    default_workflow: Option<&'static str>
) -> App<'static> {
    App::new(name)
    .about(about)
    .arg((|d| {
        let a = Arg::new("workflow")
            .help("workflow name eg. deploy/integrate")
            .short('w')
            .long("workflow")
            .takes_value(true)
            .conflicts_with("workflow_event_payload")
            .required(false);
        match d {
            Some(v) => a.default_value(v),
            None => a
        }
    })(default_workflow))
    .arg(Arg::new("workflow_context")
        .help("JSON format workflow parameter")
        .long("workflow-context")
        .short('c')
        .takes_value(true)
        .conflicts_with("workflow_event_payload")
        .required(false))
    .arg(Arg::new("workflow_event_payload")
        .help("specify CI event payload directly. deplo calculate workflow type and context from payload")
        .long("workflow-event-payload")
        .short('p')
        .takes_value(true)
        .conflicts_with_all(&["workflow_context", "workflow"])
        .required(false))
    .arg(Arg::new("release_target")
        .help("specify CI event payload directly. deplo calculate workflow type and context from payload")
        .long("release-target")
        .short('r')
        .takes_value(true)
        .required(false))
    .arg(Arg::new("env")
        .help("set adhoc environment variables for remote job")
        .long("env")
        .short('e')
        .takes_value(true)
        .multiple_occurrences(true)
        .required(false))
    .arg(Arg::new("silent")
        .help("if it set for non-interactive local execution, do not output the job's stdout/stderr.")
        .long("silent")
        .required(false))
    .arg(Arg::new("timeout")
        .help("wait timeout for remote job. if --async is set, this option is ignored")
        .long("timeout")
        .required(false)
        .takes_value(true))
    .arg(Arg::new("revision")
        .help("git ref to run the job")
        .long("rev")
        .takes_value(true)
        .required(false))
    .arg(Arg::new("remote")
        .help("if set, run the job on the correspond remote CI service")
        .long("remote")
        .required(false))
}
fn run_command_options(
    name: &'static str,
    about: &'static str,
    default_workflow: Option<&'static str>
) -> App<'static> {
    workflow_command_options(name, about, default_workflow)
        .arg(Arg::new("async")
            .help("only works with --remote, don't wait for finishing remote job")
            .long("async")
            .conflicts_with("follow_dependency")
            .required(false))
        .arg(Arg::new("follow_dependency")
            .help("if set, not only run the specified job but run dependent jobs first")
            .long("follow-dependency")
            .required(false))
        .arg(Arg::new("job")
            .help("job name")
            .index(1)
            .required(true))
        .subcommand(
            App::new("sh")
                .about("running arbiter command for environment of jobs")
                .arg(Arg::new("task")
                    .help("running command that declared in tasks directive")
                    .multiple_values(true)))
        .subcommand(
            App::new("wait")
            .about("wait specified job id. --ref,--remote,--async,--env options of parent command is ignored")
            .arg(Arg::new("job_id")
                .help("job id to wait")
                .index(1)
                .required(true)
                .takes_value(true)))
        .subcommand(
            App::new("steps")
            .about("run all steps of the job. designed to be used by deplo itself, you seldom can utilize this command"))
}

lazy_static! {
    static ref G_ALIASED_COMMANDS: AliasCommands = AliasCommands::new(hashmap!{
        "run_deploy" => (
            vec!["run"], CommandFactory::new(|name| -> App<'static> {
                run_command_options(
                    name, 
                    "run specific deploy job in Deplo.toml manually",
                    Some("deploy")
                )
            })
        ),
        "run_integrate" => (
            vec!["run"], CommandFactory::new(|name| -> App<'static> {
                run_command_options(
                    name, 
                    "run specific integrate job in Deplo.toml manually",
                    Some("integrate")
                )
            })
        ),
    }, hashmap!{
        // define aliased path like
        // "$subcommand/$subcommand_of_subcommand/$subcommand_of_subcommand_of_subcommand/..." => 
        // ["$other_subcommand", "$subcommand_of_other_subcommand", ...]
        // you also add matches for both corresponding subcommand path of G_ROOT_MATCH.
        "d" => "run_deploy",    // linked to G_ALIASED_COMMANDS.map["ci_deploy"]
        "i" => "run_integrate", // linked to G_ALIASED_COMMANDS.map["ci_integrate"]
    });
    static ref G_ROOT_MATCH: ArgMatches = App::new("deplo")
        .version(config::DEPLO_VERSION)
        .author("umegaya <iyatomi@gmail.com>")
        .about("provide integrated develop/operation UX for any automation process")
        .arg(Arg::new("config")
            .short('c')
            .long("config")
            .value_name("FILE")
            .help("Sets a custom config file path")
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
        .arg(Arg::new("verbosity")
            .short('v')
            .long("verbose")
            .help("Sets the level of verbosity")
            .takes_value(true))
        .arg(Arg::new("workdir")
            .long("workdir")
            .help("base directory that deplo runs")
            .takes_value(true))
        .subcommand(
            App::new("info")
                .about("get information about deplo")
                .subcommand(
                    App::new("version")
                        .about("get deplo version")
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
                .about("initialize deplo project. need to configure deplo.json beforehand")
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
                .about("destroy deplo configurations")
        )
        .subcommand(
            workflow_command_options(
                "boot",
                "boot deplo workflow",
                None
            )
        )
        .subcommand(
            App::new("halt")
                .about("halt and cleanup deplo workflow")
        )
        .subcommand(
            run_command_options(
                "run",
                "run specific job/workflow in Deplo.toml",
                None
            )
        )
        .subcommand(
            G_ALIASED_COMMANDS.create("d")
        )
        .subcommand(
            G_ALIASED_COMMANDS.create("i")
        )
        .subcommand(
            App::new("job")
            .subcommand(
                App::new("set-output")
                .about("set output, data passed between jobs, of current running job")
                .arg(Arg::new("key")
                    .help("key to set/get data")
                    .index(1)
                    .required(true))
                .arg(Arg::new("value")
                    .help("value to set for key")
                    .index(2)
                    .required(true)))
            .subcommand(
                App::new("output")
                .about("set output, data passed between jobs, of current running job")
                .arg(Arg::new("job")
                    .help("job name to get data. values of jobs only dependency of current job, can be retrieved")
                    .index(1)
                    .required(true))
                .arg(Arg::new("key")
                    .help("key to get value")
                    .index(2)
                    .required(true)))
        )
        .subcommand(
            App::new("ci")
                .about("control CI resources")
                .subcommand(
                    App::new("setenv")
                    .about("upload current .env contents as CI service secrets")
                )
        )
        .subcommand(
            App::new("vcs")
                .about("control VCS resources")
                .subcommand(
                    App::new("release")
                    .about("create release")
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
                    .about("upload release assets")
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
            aliased_path: vec!{}
        })
    }
    fn subcommand(&self) -> Option<(&str, Self)> {
        let (may_subcommand_name, aliased) = if self.aliased_path.len() > 0 {
            (Some(self.aliased_path[0]), true)
        } else {
            (self.matches.subcommand_name(), false)
        };
        match may_subcommand_name {
            Some(name) => {
                if aliased {
                    let mut h = self.hierarchy.clone();
                    let mut ap = self.aliased_path.clone();
                    h.push(name);
                    ap.pop();
                    Some((name, Clap::<'a>{
                        hierarchy: h,
                        matches: self.matches,
                        aliased_path: ap
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
                                aliased_path: ap
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