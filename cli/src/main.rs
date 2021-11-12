extern crate core;
#[macro_use]
extern crate lazy_static;

mod args;
mod cli;
mod command;

use core::config;
use log;

fn main() {
    let args = args::create().unwrap();
    let c = &mut config::Config::create(&args).unwrap();

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
