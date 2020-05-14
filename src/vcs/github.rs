use std::error::Error;
use std::result::Result;

use crate::config;
use crate::vcs;

pub struct Github<'a> {
    pub config: &'a config::Config<'a>
}

impl<'a> vcs::VCS<'a> for Github<'a> {
    fn new(config: &'a config::Config) -> Result<Github<'a>, Box<dyn Error>> {
        return Ok(Github::<'a> {
            config: config
        });
    }
    fn release_target(&self) -> Option<String> {
        None
    }
}
