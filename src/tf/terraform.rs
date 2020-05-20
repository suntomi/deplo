use std::error::Error;
use std::{fs, path};
use std::result::Result;

use maplit::hashmap;

use crate::config;
use crate::cloud;
use crate::shell;
use crate::tf;

pub struct Terraform<'a, S: shell::Shell<'a> = shell::Default<'a>> {
    pub config: &'a config::Config<'a>,
    pub shell: S,
}

impl<'a, S: shell::Shell<'a>> tf::Terraformer<'a> for Terraform<'a, S> {
    fn new(config: &'a config::Config) -> Result<Terraform<'a, S>, Box<dyn Error>> {
        let mut shell = S::new(config);
        shell.set_cwd(config.infra_code_dest_path())?;
        return Ok(Terraform::<'a, S> {
            config: config,
            shell
        });
    }
    fn init(&self, cloud: &Box<dyn cloud::Cloud<'a> + 'a>) -> Result<(), Box<dyn Error>> {
        let backend_config = self.config.infra_code_dest_path().join("backend");
        fs::write(&backend_config, cloud.generate_terraformer_config("terraform.backend")?)?;
        let tfvars = self.config.infra_code_dest_path().join("tfvars");
        fs::write(&tfvars, cloud.generate_terraformer_config("terraform.tfvars")?)?;
        self.shell.exec(&vec!(
            "terraform", "init", "-input=false", "-backend-config=backend",
            "-var-file=tfvars"
        ), &hashmap!{}, false)?;
        Ok(())
    }
    fn plan(&self) -> Result<(), Box<dyn Error>> {
        self.shell.exec(&vec!(
            "terraform", "plan", "--out", "/tmp/exec.tfplan",
            "-var-file=tfvars"
        ), &hashmap!{}, false)?;
        Ok(())
    }
    fn apply(&self) -> Result<(), Box<dyn Error>> {
        self.shell.exec(&vec!(
            "terraform", "apply", "/tmp/exec.tfplan"
        ), &hashmap!{}, false)?;
        Ok(())
    }
}
