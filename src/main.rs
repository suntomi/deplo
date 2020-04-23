mod config;
mod args;
mod cli;
mod command;

use log;

fn main() {
    let args = args::Args::create().unwrap();
    let c = config::Config::create(&args).unwrap();
    match cli::run(&args, &c) {
        Ok(()) => std::process::exit(0),
        Err(err) => {
            log::error!("deplo failure: {}", err);
            std::process::exit(1)
        }
    }
}
