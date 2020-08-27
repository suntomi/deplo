use std::error::Error;
use std::fs;
use std::collections::HashMap;
use std::result::Result;
use std::path::PathBuf;

use maplit::hashmap;

use crate::config;
use crate::cloud;
use crate::shell;
use crate::tf;

pub struct Terraform<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub shell: S,
}

impl<S: shell::Shell> Terraform<S> {
    fn run_env(&self) -> HashMap<&str, &str> {
        if self.config.borrow().has_debug_option("infra_debug") {
            hashmap!{ "TF_LOG" => "DEBUG" }
        } else {
            hashmap!{}
        }
    }
    fn rm(path: &PathBuf) {
        match fs::remove_file(path) {
            Ok(_) => {},
            Err(err) => { 
                log::error!(
                    "fail to cleanup for reinitialize: fail to remove {} with {:?}",
                    path.to_string_lossy(), err
                )
            },
        }
    }
    fn rmdir(path: &PathBuf) {
        match fs::remove_dir_all(path) {
            Ok(_) => {},
            Err(err) => { 
                log::error!(
                    "fail to cleanup for reinitialize: fail to remove {} with {:?}",
                    path.to_string_lossy(), err
                )
            },
        }
    }
    fn dircp(src: &PathBuf, dest: &PathBuf) -> Result<(), Box<dyn Error>> {
        match fs::metadata(dest) {
            Ok(_) => log::info!("infra setup scripts for {} already copied", dest.to_string_lossy()),
            Err(_) => {
                log::debug!("copy infra setup scripts: {}=>{}", src.to_string_lossy(), dest.to_string_lossy());
                fs_extra::dir::copy(
                    src, dest,
                    &fs_extra::dir::CopyOptions{
                        overwrite: true,
                        skip_exist: false,
                        buffer_size: 64 * 1024, //64kb
                        copy_inside: true,
                        depth: 0
                    }
                )?;
            }
        }
        Ok(())   
    }    
}

impl<S: shell::Shell> tf::Terraformer for Terraform<S> {
    fn new(config: &config::Container) -> Result<Terraform<S>, Box<dyn Error>> {
        let config_ref = config.borrow();
        let mut shell = S::new(config);
        shell.set_cwd(Some(&config_ref.infra_code_dest_root_path()))?;
        return Ok(Terraform::<S> {
            config: config.clone(),
            shell
        });
    }
    fn init(&self, main_cloud: &Box<dyn cloud::Cloud>, reinit: bool) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        if reinit {
            let cloud_provider_and_configs = config.cloud_provider_and_configs();
            for (provider_code, _) in &cloud_provider_and_configs {
                let dir_name = provider_code.to_lowercase();
                Terraform::<S>::rmdir(&config.infra_code_dest_path(provider_code));
                Terraform::<S>::rm(&config.infra_code_dest_root_path().join(format!("{}.tf", dir_name)));
            }
            for (name, _) in &config.lb {
                for (k, _) in &config.common.release_targets {
                    Terraform::<S>::rm(&config.endpoints_file_path(name, Some(k)));
                }
            }
            Terraform::<S>::rm(&config.infra_code_dest_root_path().join("tfvars"));
            Terraform::<S>::rm(&config.infra_code_dest_root_path().join("backend"));
            Terraform::<S>::rm(&config.infra_code_dest_root_path().join("main.tf"));

            let mut tfvars_list = vec!();
            for (provider_code, cloud_config) in &cloud_provider_and_configs {
                let provider_name = provider_code.to_lowercase();
                Terraform::<S>::rm(
                    &config.infra_code_dest_root_path().join(&format!("{}.tf", &provider_name))
                );
                Terraform::<S>::dircp(
                    &config.infra_code_source_path(&provider_name),
                    &config.infra_code_dest_path(&provider_name)
                )?;
                let account_name = config.account_name_from_provider_config(cloud_config).unwrap();
                let cloud = config.cloud_service(account_name)?;
                tfvars_list.push(cloud.generate_terraformer_config("terraform.tfvars")?);
                // activate cloud provider initialization,
                // by renaming $provider_name/init.tf to $provider_name.tf of infra direactory root
                fs::rename(
                    &config.infra_code_dest_path(&provider_name).join("init.tf"),
                    &config.infra_code_dest_root_path().join(&format!("{}.tf", &provider_name))
                )?;
            }
            let tfvars = config.infra_code_dest_root_path().join("tfvars");
            fs::write(&tfvars, tfvars_list.join("\n"))?;
            let backend_config = config.infra_code_dest_root_path().join("backend");
            fs::write(&backend_config, main_cloud.generate_terraformer_config("terraform.backend")?)?;
            let main_tf = config.infra_code_dest_root_path().join("main.tf");
            fs::write(&main_tf, main_cloud.generate_terraformer_config("terraform.main.tf")?)?;
        }
        self.shell.exec(&vec!(
            "terraform", "init", "-input=false", "-backend-config=backend",
            "-var-file=tfvars"
        ), &self.run_env(), false)?;
        Ok(())
    }
    fn destroy(&self, _: &Box<dyn cloud::Cloud>) {
        match self.shell.exec(&vec!(
            "terraform", "destroy", "-var-file=tfvars"
        ), &self.run_env(), false) {
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
        ), &self.run_env(), false)?;
        Ok(())
    }
    fn apply(&self) -> Result<(), Box<dyn Error>> {
        self.shell.exec(&vec!(
            "terraform", "apply", "/tmp/exec.tfplan"
        ), &self.run_env(), false)?;
        Ok(())
    }
}
