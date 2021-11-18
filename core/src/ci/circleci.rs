use std::fs;
use std::error::Error;
use std::result::Result;

use crate::config;
use crate::ci;
use crate::shell;
use crate::module;
use crate::util::{escalate,rm};

pub struct CircleCI<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub account_name: String,
    pub shell: S,
}

impl<S: shell::Shell> CircleCI<S> {
    fn generate_executor_setting<'a>(&self, job: &'a Option<config::Job>) -> String {
        return match job.container {
            Some(ref c) => format!("image: {}", c)
            None => match job.machine {
                Some(ref m) => format!("machine: {}", m),
                None => panic!("either machine or container need to specify for job")
            }
        }
    }
    fn generate_workdir_setting<'a>(&self, job: &'a Option<config::Job>) -> String {
        return job.workdir.map_or_else(|| "".to_string(), |wd| format!("workdir: {}", wd));
    }
    fn generate_checkout_steps(&self, job_name: &str, options: &Option<HashMap<String, String>>>) -> String {
        let mut checkout_opts = options.map_or_else(
            || Vec::new(), 
            |v| v.iter().map(|(k,v)| {
                return if k == "lfs" {
                    format!("{}: {}", k, v))
                } else {
                    log::warn!("deplo only support lfs options for github action checkout but {}({}) is specified", k, v)
                    ""
                }
            }).collect::<Vec<String>>()
        );
        checkout_opts.push(format!("name: {}", job_name));
        format!(
            include_str!("../../res/ci/circleci/checkout.yml.tmpl"), 
            checkout_opts = checkout_opts.join("\n"),
        )
    }
}

impl<'a, S: shell::Shell> module::Module for CircleCI<S> {
    fn prepare(&self, reinit: bool) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let repository_root = config.vcs_service()?.repository_root()?;
        let jobs = config.select_jobs("GhAction")?;
        let create_main = config.is_main_ci("GhAction");
        let circle_yml_path = format!("{}/.circleci/config.yml", repository_root);
        fs::create_dir_all(&format!("{}/.circleci", repository_root))?;
        if reinit {
            rm(&circle_yml_path);
        }
        match fs::metadata(&circle_yml_path) {
            Ok(_) => log::debug!("config file for circleci ci already created"),
            Err(_) => {
                // generate job entries
                let mut job_descs = Vec::new();
                for (name, job) in jobs {
                    job_descs.push(format!(
                        include_str!("../../res/ci/circleci/job.yml.tmpl"),
                        name = name, machine_or_container = self.generate_executor_setting(job),
                        workdir = self.generate_workdir_setting(job),
                        checkout = self.generate_checkout_steps(name, job.options),
                    ))
                }
                // sync dotenv secrets with ci system
                config.parse_dotenv(|k,v| (self as &dyn ci::CI).set_secret(k, v))?;
                fs::write(&circle_yml_path, format!(
                    include_str!("../../rsc/ci/circleci/config.yml.tmpl"),
                    image = config.common.deplo_image, tag = config::DEPLO_GIT_HASH,
                    workflow = if create_main {
                        include_str!("../../res/ci/circleci/workflow.yml.tmpl")
                    } else {
                        ""
                    },
                    jobs = job_descs.join("\n")
                ))?;
            }
        }
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
    fn run_job(&self, job_name: &str) -> Result<String, Box<dyn Error>> {
        Ok("".to_string())
    }
    fn wait_job(&self, job_id: &str) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn wait_job_by_name(&self, job_name: &str) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn set_secret(&self, key: &str, val: &str) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let token = match &config.ci_config(&self.account_name) {
            config::CIConfig::CircleCI { key, workflow:_ } => { key },
            config::CIConfig::GhAction{..} => { 
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
        ), shell::no_env(), true)?.parse::<u32>()?;
        if status >= 200 && status < 300 {
            Ok(())
        } else {
            return escalate!(Box::new(ci::CIError {
                cause: format!("fail to set secret to CircleCI CI with status code:{}", status)
            }));
        }
    }
}
