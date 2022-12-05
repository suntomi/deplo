use std::error::Error;

use core::config;
use core::shell;

use crate::args;
use crate::command;
use crate::util::{escalate};

pub struct Job<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub shell: S
}
impl<S: shell::Shell> Job<S> {
    fn output<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let job = args.value_or_die("job");
        let key = args.value_or_die("key");
        match config.jobs.user_output(&config, job, key)? {
            Some(v) => println!("{}", v),
            None => {}
        }
        Ok(())
    }
    fn set_output<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let key = args.value_or_die("key");
        let value = args.value_or_die("value");
        config.jobs.set_user_output(&config, key, value)?;
        Ok(())
    }
    fn steps<A: args::Args>(&self, args: &A) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let parent_workflow = args.value_or_die("parent_workflow");
        let job = args.value_or_die("job");
        let task = args.value_of("task");
        let workflow = config::runtime::Workflow::with_payload(parent_workflow)?;
        config.jobs.run_steps(&config, &self.shell, &workflow, job, task)?;
        Ok(())
    }
}

impl<S: shell::Shell, A: args::Args> command::Command<A> for Job<S> {
    fn new(config: &config::Container) -> Result<Job<S>, Box<dyn Error>> {
        return Ok(Job::<S> {
            config: config.clone(),
            shell: S::new(config)
        });
    }
    fn run(&self, args: &A) -> Result<(), Box<dyn Error>> {
        match args.subcommand() {
            Some(("output", subargs)) => return self.output(&subargs),
            Some(("set-output", subargs)) => return self.set_output(&subargs),
            Some(("run-steps", subargs)) => return self.steps(&subargs),
            Some((name, _)) => return escalate!(args.error(
                &format!("no such subcommand: [{}]", name) 
            )),
            None => return escalate!(args.error("no subcommand specified"))
        }
    }
}
