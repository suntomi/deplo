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

use log;

fn main() {
    let args = args::create().unwrap();
    let c = config::Config::create(&args).unwrap();
    match cli::run(&args, &c) {
        Ok(()) => std::process::exit(0),
        Err(err) => {
            log::error!("command failure: {:?}", err);
            std::process::exit(1)
        }
    }
}
