use std::error::Error;
use std::fs;
use std::collections::HashMap;
use std::result::Result;
use std::path::PathBuf;

use maplit::hashmap;

use crate::config;
use crate::cloud;
use crate::module;
use crate::shell;
use crate::tf;
use crate::util::escalate;

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
            Ok(d) => log::info!(
                "infra setup scripts for {}({:?}) already copied",
                dest.to_string_lossy(), fs::canonicalize(&dest)
            ),
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
    fn write_file<F>(dest: &PathBuf, make_contents: F) -> Result<bool, Box<dyn Error>> 
    where F: Fn () -> Result<String, Box<dyn Error>> {
        match fs::metadata(dest) {
            Ok(_) => {
                log::debug!(
                    "infra config file {}({:?}) already exists",
                    dest.to_string_lossy(), fs::canonicalize(&dest)
                );
                Ok(false)
            },
            Err(_) => {
                log::debug!("create infra config file {}", dest.to_string_lossy());
                fs::write(&dest, &make_contents()?)?;
                Ok(true)
            }
        }
    }
}

impl<S: shell::Shell> module::Module for Terraform<S> {
    fn prepare(&self, reinit: bool) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let main_cloud = config.cloud_service("default")?;
        let cloud_provider_and_configs = config.cloud_provider_and_configs();
        if reinit {
            for (provider_code, _) in &cloud_provider_and_configs {
                let dir_name = provider_code.to_lowercase();
                Terraform::<S>::rmdir(&config.infra_code_dest_path(provider_code));
                Terraform::<S>::rm(&config.infra_code_dest_root_path().join(format!("{}.tf", dir_name)));
            }
            Terraform::<S>::rm(&config.infra_code_dest_root_path().join("tfvars"));
            Terraform::<S>::rm(&config.infra_code_dest_root_path().join("backend"));
            Terraform::<S>::rm(&config.infra_code_dest_root_path().join("main.tf"));
        }

        // copy terraform scripts for each cloud provider 
        for (provider_code, cloud_config) in &cloud_provider_and_configs {
            let provider_name = provider_code.to_lowercase();
            Terraform::<S>::dircp(
                &config.infra_code_source_path(&provider_name),
                &config.infra_code_dest_path(&provider_name)
            )?;
            // activate cloud provider initialization,
            // by renaming $provider_name/init.tf to $provider_name.tf of infra direactory root
            let tf_entry_file = &config.infra_code_dest_root_path().join(&format!("{}.tf", &provider_name));
            match fs::metadata(tf_entry_file) {
                Ok(_) => log::debug!("tf entry point already moved {}", tf_entry_file.to_string_lossy()),
                Err(_) => fs::rename(
                    &config.infra_code_dest_path(&provider_name).join("init.tf"),
                    &config.infra_code_dest_root_path().join(&format!("{}.tf", &provider_name))
                )?
            }
        }
        log::debug!("generate settings files");
        // generate setting files
        let tfvars = config.infra_code_dest_root_path().join("tfvars");
        let backend_config = config.infra_code_dest_root_path().join("backend");
        let main_tf = config.infra_code_dest_root_path().join("main.tf");

        if  // check any of these files are not exists
            !fs::metadata(&tfvars).is_ok() ||
            !fs::metadata(&backend_config).is_ok() ||
            !fs::metadata(&main_tf).is_ok() 
        {
            // create bucket which contains terraform state
            let config::TerraformerConfig::Terraform {
                backend:_,
                backend_bucket,
                resource_prefix:_
            } = &config.cloud.terraformer;        
            main_cloud.create_bucket(backend_bucket, &cloud::CreateBucketOption{ region: None })?;

            // generate setting files
            // tfvars for each cloud provider setup scripts
            Terraform::<S>::write_file(&tfvars, || {
                // generate tfvars for each cloud provider, from config
                let mut tfvars_list = vec!(format!(
                    "\
                        resource_prefix = \"{}\"\n\
                        envs = [\"{}\"]\n\
                    ",
                    config.cloud.resource_prefix().as_ref().unwrap_or(&config.project_namespace().to_string()), 
                    config.common.release_targets
                        .keys().map(|s| &**s)
                        .collect::<Vec<&str>>().join(r#"",""#)
                ));
                for (provider_code, cloud_config) in &cloud_provider_and_configs {
                    let account_name = config.account_name_from_provider_config(cloud_config).unwrap();
                    let cloud = config.cloud_service(account_name)?;
                    tfvars_list.push(cloud.generate_terraformer_config("terraform.tfvars")?);
                }
                Ok(tfvars_list.join("\n"))
            })?;
            // backend bucket config for terraform state
            Terraform::<S>::write_file(&backend_config, 
                || { main_cloud.generate_terraformer_config("terraform.backend") })?;
            // entry point of all terraform scripts
            Terraform::<S>::write_file(&main_tf, 
                || { main_cloud.generate_terraformer_config("terraform.main.tf") })?;
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
    fn init(&self) -> Result<(), Box<dyn Error>> {
        self.shell.exec(&vec!(
            "terraform", "init", "-input=false", "-backend-config=backend",
            "-var-file=tfvars"
        ), &self.run_env(), false)?;
        Ok(())
    }
    fn destroy(&self) -> Result<(), Box<dyn Error>> {
        match self.shell.exec(&vec!(
            "terraform", "destroy", "-var-file=tfvars"
        ), &self.run_env(), false) {
            Ok(_) => {},
            Err(err) => {
                log::error!("\
                    destroy infra fail with {:?}, \
                    check undeleted resources and manually cleanup\
                ", err);
                return escalate!(Box::new(err));
            }
        }
        let config = self.config.borrow();
        let main_cloud = config.cloud_service("default")?;
        let config::TerraformerConfig::Terraform {
            backend:_,
            backend_bucket,
            resource_prefix:_
        } = &config.cloud.terraformer;        
        main_cloud.delete_bucket(backend_bucket)
    }
    fn rclist(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let r = self.shell.output_of(&vec!(
            "terraform", "state", "list", "-no-color"
        ), &self.run_env())?;
        Ok(r.split('\n').map(|s| s.to_string()).collect())
    }
    fn rm(&self, path: &str) -> Result<(), Box<dyn Error>> {
        self.shell.exec(&vec!(
            "terraform", "state", "rm", path
        ), &self.run_env(), false)?;
        Ok(())
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
        ", addr_and_key.0), shell::no_env())?;
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
