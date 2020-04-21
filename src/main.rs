use clap::{Arg, App};
mod config;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

fn main() {
    let matches = App::new("deplo")
        .version(VERSION)
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
            App::new("gcloud")
                .about("wrap gcloud to dryrun")
                .arg(Arg::with_name("input")
                    .help("the file to add")
                    .index(1)
                    .required(true))
        )
        .get_matches();

    let c = config::Config::create(matches).unwrap();

    println!("{}, {}, {}", c.common.deplo_image, c.cli.dryrun, VERSION)
}
