#[macro_use]
extern crate lazy_static;

mod config;
mod args;
mod cli;
mod command;
mod shell;
mod cloud;
mod vcs;
mod endpoints;
mod tf;
mod lb;
mod ci;
mod util;
mod plan;
mod module;
mod builder;

use log;

fn main() {
    let args = args::create().unwrap();
    let c = &mut config::Config::create(&args).unwrap();
    match cli::prerun(&args, c) {
        Ok(r) => if r { std::process::exit(0) },
        Err(err) => {
            log::error!("command failure: {}", err);
            std::process::exit(1)
        }
    }
    match cli::run(
        &args,
        config::Config::setup(c, &args).unwrap()
    ) {
        Ok(()) => std::process::exit(0),
        Err(err) => {
            log::error!("command failure: {}", err);
            std::process::exit(1)
        }
    }
}
