use std::error::Error;

use crate::args;
use crate::config;

pub fn run(args: &args::Args, config: &config::Config) -> Result<i32, Box<dyn Error>> {
    println!("{}, {}", config.common.deplo_image, config.cli.dryrun);
    return Ok(0);
}

