extern crate core;

mod cli;
mod command;
mod util;

use core::args;
use core::config;
use log;

fn main() {
    let args = args::create().expect("fail to process cli args");
    let r = &mut config::Config::create(&args);
    let c = match r {
        Ok(c) => c,
        Err(err) => {
            println!("fail to create config: {}", err);
            std::process::exit(1);
        }
    };

    match cli::run(&args, c){
        Ok(()) => std::process::exit(0),
        Err(err) => {
            log::error!("command failure: {}", err);
            std::process::exit(1)
        }
    }
}
