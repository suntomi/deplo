use std::error::Error;

use super::args;

pub trait Command {
    fn run(&self, args: &args::Args) -> Result<(), Box<dyn Error>>;
}

// subcommands
pub mod gcloud;
