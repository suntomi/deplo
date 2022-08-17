extern crate core;

mod cli;
mod command;
mod util;

use core::args;
use core::config;
use log;

fn main() {
    let args = args::create().expect("fail to process cli args");
    let c = &mut config::Config::create(&args).expect("fail to create config");

    match cli::run(&args, c){
        Ok(()) => std::process::exit(0),
        Err(err) => {
            log::error!("command failure: {}", err);
            std::process::exit(1)
        }
    }
}
