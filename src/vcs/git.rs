use std::error::Error;

use maplit::hashmap;

use crate::config;
use crate::shell;

pub struct Git<'a, S: shell::Shell<'a> = shell::Default<'a>> {
    config: &'a config::Config<'a>,
    shell: S,
}

impl<'a, S: shell::Shell<'a>> Git<'a, S> {
    pub fn new(config: &'a config::Config<'a>) -> Git<'a, S> {
        return Git::<'a, S> {
            config,
            shell: S::new(config)
        }
    }
    pub fn current_branch(&self) -> Result<String, Box<dyn Error>> {
        return self.shell.output_of(&vec!(
            "git", "symbolic-ref" , "--short", "HEAD"
        ), &hashmap!{});
    }
}
