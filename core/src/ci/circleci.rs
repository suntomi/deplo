use std::fs;
use std::error::Error;
use std::result::Result;
use std::collections::{HashMap};

use maplit::hashmap;

use crate::config;
use crate::ci;
use crate::shell;
use crate::module;
use crate::util::{escalate,MultilineFormatString,rm,maphash};

pub struct CircleCI<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub account_name: String,
    pub shell: S,
}

impl<S: shell::Shell> CircleCI<S> {
    fn generate_entrypoint<'a>(&self, _: &'a config::Config) -> Vec<String> {
        include_str!("../../res/ci/circleci/entrypoint.yml.tmpl")
            .to_string().split("\n").map(|s| s.to_string()).collect()
    }
    fn generate_executor_setting<'a>(&self, runner: &'a config::Runner) -> String {
        return match runner {
            config::Runner::Machine{ os, image, class, .. } => format!(
                include_str!("../../res/ci/circleci/machine.yml.tmpl"), 
                image = match image {
                    Some(v) => v,
                    None => match os {
                        config::RunnerOS::Linux => "ubuntu-latest",
                        config::RunnerOS::Windows => "macos-latest",
                        config::RunnerOS::MacOS => "windows-latest",
                    },
                },
                class = match class {
                    Some(v) => format!("resource_class: {}", v),
                    None => "".to_string(),
                }
            ),
            config::Runner::Container{ image } => format!("image: {}", image),
        }
    }
    fn generate_workdir_setting<'a>(&self, job: &'a config::Job) -> String {
        return job.workdir.as_ref().map_or_else(|| "".to_string(), |wd| format!("workdir: {}", wd));
    }
    fn generate_checkout_steps(&self, _: &str, options: &Option<HashMap<String, String>>) -> String {
        let mut checkout_opts = options.as_ref().map_or_else(
            || Vec::new(), 
            |v| v.iter().map(|(k,v)| {
                return if k == "lfs" {
                    format!("{}: {}", k, v)
                } else {
                    log::warn!("deplo only support lfs options for github action checkout but {}({}) is specified", k, v);
                    "".to_string()
                }
            }).collect::<Vec<String>>()
        );
        checkout_opts.push(format!("opts_hash: {}", options.as_ref().map_or_else(
            || "".to_string(), 
            |v| maphash(v)
        )));
        format!(
            include_str!("../../res/ci/circleci/checkout.yml.tmpl"), 
            checkout_opts = MultilineFormatString{
                strings: &checkout_opts,
                postfix: None
            }
        )
    }
}

impl<'a, S: shell::Shell> module::Module for CircleCI<S> {
    fn prepare(&self, reinit: bool) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let repository_root = config.vcs_service()?.repository_root()?;
        let jobs = config.enumerate_jobs();
        let create_main = config.is_main_ci("GhAction");
        // TODO_PATH: use Path to generate path of /.circleci/...
        let circle_yml_path = format!("{}/.circleci/config.yml", repository_root);
        fs::create_dir_all(&format!("{}/.circleci", repository_root))?;
        let previously_no_file = !rm(&circle_yml_path);
        // generate job entries
        let mut job_descs = Vec::new();
        for (names, job) in &jobs {
            let name = format!("{}-{}", names.0, names.1);
            let lines = format!(
                include_str!("../../res/ci/circleci/job.yml.tmpl"),
                full_name = name, kind = names.0, name = names.1, 
                machine_or_container = self.generate_executor_setting(&job.runner),
                workdir = self.generate_workdir_setting(job),
                checkout = self.generate_checkout_steps(&name, &job.checkout),
            ).split("\n").map(|s| s.to_string()).collect::<Vec<String>>();
            job_descs = job_descs.into_iter().chain(lines.into_iter()).collect();
        }
        if previously_no_file || reinit {
            // sync dotenv secrets with ci system
            config.parse_dotenv(|k,v| {
                (self as &dyn ci::CI).set_secret(k, v)?;
                log::info!("set secret value of {}", k);
                Ok(())
            })?;
        }
        fs::write(&circle_yml_path, format!(
            include_str!("../../res/ci/circleci/main.yml.tmpl"),
            image = config.deplo_image(), tag = config::DEPLO_GIT_HASH,
            entrypoint = MultilineFormatString{ 
                strings: &(if create_main { self.generate_entrypoint(&config) } else { vec![] }),
                postfix: None
            },
            jobs = MultilineFormatString{ 
                strings: &job_descs,
                postfix: None
            }
        ))?;
        //TODO: we need to provide the way to embed user defined circle ci configuration with our generated config.yml
        Ok(())
    }
}

impl<'a, S: shell::Shell> ci::CI for CircleCI<S> {
    fn new(config: &config::Container, account_name: &str) -> Result<CircleCI<S>, Box<dyn Error>> {
        return Ok(CircleCI::<S> {
            config: config.clone(),
            account_name: account_name.to_string(),
            shell: S::new(config),
        });
    }
    fn kick(&self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn pull_request_url(&self) -> Result<Option<String>, Box<dyn Error>> {
        match std::env::var("CIRCLE_PULL_REQUEST") {
            Ok(v) => Ok(Some(v)),
            Err(e) => {
                match e {
                    std::env::VarError::NotPresent => Ok(None),
                    _ => return escalate!(Box::new(e))
                }
            }
        }
    }
    fn mark_job_executed(&self, job_name: &str) -> Result<(), Box<dyn Error>> {
        if config::Config::is_running_on_ci() {
            fs::create_dir_all("/tmp/deplo/marked_jobs")?;
            fs::write(format!("/tmp/deplo/marked_jobs/{}", job_name), "")?;
        } else {
            self.config.borrow().run_job_by_name(&self.shell, job_name)?;
        }
        Ok(())
    }
    fn run_job(&self, _: &str) -> Result<String, Box<dyn Error>> {
        log::warn!("TODO: implement run_job for circleci");
        Ok("".to_string())
    }
    fn wait_job(&self, _: &str) -> Result<(), Box<dyn Error>> {
        log::warn!("TODO: implement wait_job for circleci");
        Ok(())
    }
    fn wait_job_by_name(&self, _: &str) -> Result<(), Box<dyn Error>> {
        log::warn!("TODO: implement wait_job_by_name for circleci");
        Ok(())
    }
    fn job_env(&self) -> HashMap<&str, String> {
        let config = self.config.borrow();
        return hashmap!{
            //TODO_CI: need to get pr URL value on local execution
            "DEPLO_CI_PULL_REQUEST_URL" => std::env::var("CIRCLE_PULL_REQUEST").unwrap_or_else(|_| "".to_string()),
            "DEPLO_CI_TYPE" => "CircleCI".to_string(),
            "DEPLO_CI_CURRENT_SHA" => std::env::var("CIRCLE_SHA1").unwrap_or_else(
                |_| config.vcs_service().unwrap().commit_hash(None).unwrap()
            ),
        }
    }
    fn set_secret(&self, key: &str, val: &str) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let token = match &config.ci_config(&self.account_name) {
            config::CIAccount::CircleCI { key } => { key },
            config::CIAccount::GhAction{..} => { 
                return escalate!(Box::new(ci::CIError {
                    cause: "should have circleci CI config but ghaction config provided".to_string()
                }));
            }
        };
        let json = format!("{{\"name\":\"{}\",\"value\":\"{}\"}}", key, val);
        let user_and_repo = config.vcs_service()?.user_and_repo()?;
        let status = self.shell.exec(&vec!(
            "curl", "-X", "POST", "-u", &format!("{}:", token),
            &format!(
                "https://circleci.com/api/v2/project/gh/{}/{}/envvar",
                user_and_repo.0, user_and_repo.1
            ),
            "-H", "Content-Type: application/json",
            "-H", "Accept: application/json",
            "-d", &json, "-w", "%{http_code}", "-o", "/dev/null"
        ), shell::no_env(), shell::no_cwd(), true)?.parse::<u32>()?;
        if status >= 200 && status < 300 {
            Ok(())
        } else {
            return escalate!(Box::new(ci::CIError {
                cause: format!("fail to set secret to CircleCI CI with status code:{}", status)
            }));
        }
    }
}
