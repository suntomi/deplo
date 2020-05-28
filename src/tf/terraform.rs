use std::error::Error;
use std::fs;
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
        shell.set_cwd(Some(&config.infra_code_dest_path()))?;
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
    fn destroy(&self, _: &Box<dyn cloud::Cloud<'a> + 'a>) {
        match self.shell.exec(&vec!(
            "terraform", "destroy", "-var-file=tfvars"
        ), &hashmap!{}, false) {
            Ok(_) => {},
            Err(err) => {
                log::error!("\
                    destroy infra fail with {:?}, \
                    check undeleted resources and manually cleanup\
                ", err);
            }
        }
    }
    fn rclist(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let r = self.shell.output_of(&vec!(
            "terraform", "state", "list", "-no-color"
        ), &hashmap!{})?;
        Ok(r.split('\n').map(|s| s.to_string()).collect())
    }
    fn eval(&self, path: &str) -> Result<String, Box<dyn Error>> {
        let parsed: Vec<&str> = path.split("@").collect();
        let addr_and_key = if parsed.len() > 1 {
            (parsed[1], parsed[0])
        } else {
            (parsed[0], "")
        };
        let r = self.shell.eval_output_of(&format!("\
            terraform state show -no-color '{}'
        ", addr_and_key.0), &hashmap!{})?;
        if addr_and_key.1.is_empty() {
            Ok(r)
        } else {
            let re = regex::Regex::new(&format!(r#"{}\s+=\s+"([^"]*)""#, addr_and_key.1)).unwrap();
            return match re.captures(&r) {
                Some(c) => Ok(c.get(1).map_or("", |m| m.as_str()).to_string()),
                None => Ok("".to_string())
            }
        }
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
