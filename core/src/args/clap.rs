use std::collections::{HashMap};
use std::error::Error;
use std::result::Result;

use clap::{Command, Arg, ArgMatches};
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
    factory: fn (&'static str) -> Command,
}
impl CommandFactory {
    fn new(f: fn (&'static str) -> Command) -> Self {
        Self { factory: f }
    }
    fn create(&self, name: &'static str) -> Command {
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
    fn create(&self, name: &'static str) -> Command {
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
) -> Command {
    Command::new(name)
    .about(about)
    .arg((|d| {
        let a = Arg::new("workflow")
            .help("workflow name eg. deploy/integrate")
            .short('w')
            .long("workflow")
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
        .conflicts_with("workflow_event_payload")
        .required(false))
    .arg(Arg::new("workflow_event_payload")
        .help("specify CI event payload directly. deplo calculate workflow type and context from payload")
        .long("workflow-event-payload")
        .short('p')
        .conflicts_with_all(&["workflow_context", "workflow"])
        .required(false))
    .arg(Arg::new("release_target")
        .help("specify CI event payload directly. deplo calculate workflow type and context from payload")
        .long("release-target")
        .short('r')
        .required(false))
    .arg(Arg::new("env")
        .help("set adhoc environment variables for remote job")
        .long("env")
        .short('e')
        .action(clap::ArgAction::Append)
        .required(false))
    .arg(Arg::new("silent")
        .help("if it set for non-interactive local execution, do not output the job's stdout/stderr.")
        .long("silent")
        .action(clap::ArgAction::SetTrue)
        .required(false))
    .arg(Arg::new("timeout")
        .help("wait timeout for remote job. if --async is set, this option is ignored")
        .long("timeout")
        .required(false))
    .arg(Arg::new("revision")
        .help("git ref or sha to run the job")
        .long("rev")
        .required(false))
    .arg(Arg::new("remote")
        .help("if set, run the job on the correspond remote CI service")
        .long("remote")
        .action(clap::ArgAction::SetTrue)
        .required(false))
    .arg(Arg::new("debug")
        .help(r#"if set, run debugger after finishing job. 
there are 3 options.
'always'(or 'a'): always start debugger after job,
'failure'(or 'f'): start debugger only if job failed
'never'( or 'n'): never start debugger"#)
        .short('d')
        .long("debug")
        .value_parser(["always", "failure", "never", "a", "f", "n"])
        .required(false))
    .arg(Arg::new("debug-job")
        .help(r#"by default, deplo try to stop on all job which matches condition specified with --debug option, except deplo-main/deplo-halt.
if the option is set, deplo run debugger after finishing job only that specified as value.
normally job name. if you want to debug deplo-main/deplo-halt, specify these names for the argument"#)
        .short('j')
        .long("debug-job")
        .required(false))
}
fn run_command_options(
    name: &'static str,
    about: &'static str,
    default_workflow: Option<&'static str>
) -> Command {
    workflow_command_options(name, about, default_workflow)
        .arg(Arg::new("async")
            .help("only works with --remote, don't wait for finishing remote job")
            .long("async")
            .conflicts_with("follow_dependency")
            .action(clap::ArgAction::SetTrue)
            .required(false))
        .arg(Arg::new("follow_dependency")
            .help("if set, not only run the specified job but run dependent jobs first")
            .long("follow-dependency")
            .action(clap::ArgAction::SetTrue)
            .required(false))
        .arg(Arg::new("job")
            .help("job name")
            .index(1)
            .required(true))
        .subcommand(
            Command::new("sh")
                .about("running arbiter command for environment of jobs")
                .arg(Arg::new("task")
                    .help("running command that declared in tasks directive")
                    .num_args(1..)))
        .subcommand(
            Command::new("wait")
            .about("wait specified job id. --ref,--remote,--async,--env options of parent command is ignored")
            .arg(Arg::new("job_id")
                .help("job id to wait")
                .index(1)
                .required(true)))
}

lazy_static! {
    static ref G_ALIASED_COMMANDS: AliasCommands = AliasCommands::new(hashmap!{
        "run_deploy" => (
            vec!["run"], CommandFactory::new(|name| -> Command {
                run_command_options(
                    name, 
                    "run specific deploy job in Deplo.toml manually",
                    Some("deploy")
                )
            })
        ),
        "run_integrate" => (
            vec!["run"], CommandFactory::new(|name| -> Command {
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
    static ref G_ROOT_MATCH: ArgMatches = Command::new("deplo")
        .version(config::DEPLO_VERSION)
        .author("umegaya <iyatomi@gmail.com>")
        .about("provide integrated develop/operation UX for any automation process")
        .arg(Arg::new("config")
            .short('c')
            .long("config")
            .value_name("FILE")
            .help("Sets a custom config file path"))
        .arg(Arg::new("debug")
            .short('d')
            .long("debug")
            .value_name("KEY(=VALUE),...")
            .help("Activate debug feature\n\
                    TODO: add some debug flags")
            .num_args(1..)
            .action(clap::ArgAction::Append))
        .arg(Arg::new("dotenv")
            .short('e')
            .long("dotenv")
            .value_name(".ENV FILE OR TEXT")
            .help("specify .env file path or .env file content directly"))
        .arg(Arg::new("verbosity")
            .short('v')
            .long("verbose")
            .help("Sets the level of verbosity"))
        .arg(Arg::new("workdir")
            .long("workdir")
            .help("base directory that deplo runs"))
        .subcommand(
            Command::new("info")
                .about("get information about deplo")
                .subcommand(
                    Command::new("version")
                        .about("get deplo version")
                        .arg(Arg::new("output")
                            .help("output format")
                            .short('o')
                            .long("output")
                            .value_parser(["plain", "json"])
                            .required(false))
                )
        )
        .subcommand(
            Command::new("init")
                .about("initialize deplo project. need to configure deplo.json beforehand")
                .arg(Arg::new("reinit")
                    .long("reinit")
                    .help("initialize component")
                    .required(false)
                    .value_parser(["ci", "vcs", "all"]))
        )
        .subcommand(
            Command::new("destroy")
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
            workflow_command_options(
                "halt",
                "halt deplo workflow",
                None
            )
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
            Command::new("job")
            .subcommand(
                Command::new("set-output")
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
                Command::new("output")
                .about("set output, data passed between jobs, of current running job")
                .arg(Arg::new("job")
                    .help("job name to get data. values of jobs only dependency of current job, can be retrieved")
                    .index(1)
                    .required(true))
                .arg(Arg::new("key")
                    .help("key to get value")
                    .index(2)
                    .required(true)))
            .subcommand(
                Command::new("run-steps")
                .about("run all steps of the job. designed to be used by deplo itself, you seldom can utilize this command")
                .arg(Arg::new("job")
                    .help("job name to get data. values of jobs only dependency of current job, can be retrieved")
                    .index(1)
                    .required(true))
                .arg(Arg::new("parent_workflow")
                    .help("runtime workflow config of parent deplo process")
                    .short('p')
                    .long("parent_workflow")
                    .required(true))
                .arg(Arg::new("task")
                    .help("task name of job")
                    .long("task")
                    .required(false)))
        )
        .subcommand(
            Command::new("ci")
                .about("control CI resources")
                .subcommand(
                    Command::new("setenv")
                    .about("upload current .env contents as CI service secrets")
                )
                .subcommand(
                    Command::new("getenv")
                    .about("generate .env format file that contains all secrets/vars")
                    .arg(Arg::new("output")
                        .help("output file path")
                        .short('o')
                        .long("out")
                        .required(true))
                )
                .subcommand(
                    Command::new("restore-cache")
                    .about("restore cached repository to correct state")
                    .arg(Arg::new("submodules")
                        .help("task name of job")
                        .short('s')
                        .long("submodules")
                        .action(clap::ArgAction::SetTrue)
                        .required(false))
                )
                .subcommand(
                    Command::new("token")
                    .about("generate temporary token of CI service like oidc jwt")
                    .subcommand(
                        Command::new("oidc")
                        .about("generate oidc web identity token for current CI service")
                        .arg(Arg::new("audience")
                            .help("aud parameter of generated jwt")
                            .short('a')
                            .long("aud")
                            .required(true))
                        .arg(Arg::new("output")
                            .help("file path to output generated token")
                            .short('o')
                            .long("out")
                            .required(true))
                    )
                )
        )
        .subcommand(
            Command::new("vcs")
                .about("control VCS resources")
                .subcommand(
                    Command::new("release")
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
                        .action(clap::ArgAction::Append))
                )
                .subcommand(
                    Command::new("release-assets")
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
                        .long("replace")
                        .action(clap::ArgAction::SetTrue))
                    .arg(Arg::new("option")
                        .help("option for release creation.\n\
                                -o name=$release_asset_name\n\
                                -o content-type=$content_type_of_asset\n\
                                TODO: implement more options")
                        .short('o')
                        .action(clap::ArgAction::Append))
                )
                .subcommand(
                    Command::new("pr")
                    .about("control pull request")
                    .subcommand(
                        Command::new("merge")
                        .about("merge pull request")
                        .arg(Arg::new("url")
                            .help("URL of the pull request")
                            .index(1)
                            .required(true))
                        .arg(Arg::new("option")
                            .help("option for pull request merge.\n\
                                    -o $key=$value\n\
                                    for github, body options of https://docs.github.com/en/rest/pulls/pulls?apiVersion=2022-11-28#merge-a-pull-request can be specified.\n\
                                    plus, -o auto_merge=true to enable auto merge.\n\
                                          -o message=$text to post comment to pull request.\n\
                                          -o approve=true to approve the pull request.\n\
                                    TODO: for gitlab")
                            .short('o')
                            .action(clap::ArgAction::Append))
                    )
                    .subcommand(
                        Command::new("close")
                        .about("close pull request")
                        .arg(Arg::new("url")
                            .help("URL of the pull request")
                            .index(1)
                            .required(true))
                        .arg(Arg::new("option")
                            .help("option for pull request merge.\n\
                                    -o message=$text to post comment to pull request.")
                            .short('o')
                            .action(clap::ArgAction::Append))         
                    )
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
    fn get_flag(&self, name: &str) -> bool {
        return self.matches.get_flag(name);
    }
    fn value_of(&self, name: &str) -> Option<&str> {
        match self.matches.try_get_one::<String>(name) {
            Ok(v) => match v {
                Some(v) => Some(v.as_str()),
                None => None
            }
            Err(_) => None
        }
    }
    fn values_of(&self, name: &str) -> Option<Vec<&str>> {
        match self.matches.try_get_many::<String>(name) {
            Ok(vs) => match vs {
                Some(vs) => Some(vs.map(|v| v.as_str()).collect()),
                None => None
            }
            Err(_) => None
        }
    }
    fn command_path(&self) -> &Vec<&str> {
        &self.hierarchy
    }
}