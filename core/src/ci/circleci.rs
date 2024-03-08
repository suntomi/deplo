use std::fs;
use std::error::Error;
use std::result::Result;
use std::collections::{HashMap};

use maplit::hashmap;

use crate::config;
use crate::ci::{self, CheckoutOption};
use crate::shell;
use crate::util::{escalate,MultilineFormatString,rm};

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
    fn generate_executor_setting<'a>(&self, runner: &'a config::job::Runner) -> String {
        return match runner {
            config::job::Runner::Machine{ os, image, class, .. } => format!(
                include_str!("../../res/ci/circleci/machine.yml.tmpl"), 
                image = match image {
                    Some(v) => v.resolve(),
                    None => (match os {
                        config::job::RunnerOS::Linux => "ubuntu-latest",
                        config::job::RunnerOS::Windows => "macos-latest",
                        config::job::RunnerOS::MacOS => "windows-latest",
                    }).to_string(),
                },
                class = match class {
                    Some(v) => format!("resource_class: {}", v),
                    None => "".to_string(),
                }
            ),
            config::job::Runner::Container{ image, .. } => format!("image: {}", image),
        }
    }
    fn generate_workdir_setting<'a>(&self, job: &'a config::job::Job) -> String {
        return job.workdir.as_ref().map_or_else(|| "".to_string(), |wd| format!("workdir: {}", wd));
    }
    fn generate_checkout_steps(&self, _: &str, options: &Option<config::job::CheckoutOption>) -> String {
        let mut checkout_opts = options.as_ref().map_or_else(|| Vec::new(), |v| v.opt_str());
        checkout_opts.push(format!("opts_hash: {}", options.as_ref().map_or_else(|| "".to_string(), |v| v.hash())));
        format!(
            include_str!("../../res/ci/circleci/checkout.yml.tmpl"), 
            checkout_opts = MultilineFormatString{
                strings: &checkout_opts,
                postfix: None
            }
        )
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
    fn runs_on_service(&self) -> bool {
        std::env::var("CIRCLE_SHA1").is_ok()
    }
    fn restore_cache(&self, _submodule: bool) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn generate_config(&self, reinit: bool) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let repository_root = config.modules.vcs().repository_root()?;
        let jobs = config.jobs.as_map();
        let create_main = config.ci.is_main("GhAction");
        // TODO_PATH: use Path to generate path of /.circleci/...
        let circle_yml_path = format!("{}/.circleci/config.yml", repository_root);
        fs::create_dir_all(&format!("{}/.circleci", repository_root))?;
        let previously_no_file = !rm(&circle_yml_path);
        // generate job entries
        let mut job_descs = Vec::new();
        for (name, job) in jobs {
            let lines = format!(
                include_str!("../../res/ci/circleci/job.yml.tmpl"),
                name = name,
                machine_or_container = self.generate_executor_setting(&job.runner),
                workdir = self.generate_workdir_setting(job),
                checkout = self.generate_checkout_steps(&name, &job.checkout),
            ).split("\n").map(|s| s.to_string()).collect::<Vec<String>>();
            job_descs = job_descs.into_iter().chain(lines.into_iter()).collect();
        }
        if previously_no_file || reinit {
            // sync dotenv secrets with ci system
            for (k, v) in &config::secret::vars()? {
                (self as &dyn ci::CI).set_secret(k, v)?;
                log::debug!("set secret value of {}", k);
            }
        }
        fs::write(&circle_yml_path, format!(
            include_str!("../../res/ci/circleci/main.yml.tmpl"),
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
    fn pr_url_from_env(&self) -> Result<Option<String>, Box<dyn Error>> {
        match std::env::var("CIRCLE_PULL_REQUEST") {
            Ok(v) => if v.is_empty() { Ok(None) } else { Ok(Some(v)) },
            Err(e) => {
                match e {
                    std::env::VarError::NotPresent => Ok(None),
                    _ => return escalate!(Box::new(e))
                }
            }
        }
    }
    fn schedule_job(&self, job_name: &str) -> Result<(), Box<dyn Error>> {
        fs::create_dir_all("/tmp/deplo/marked_jobs")?;
        fs::write(format!("/tmp/deplo/marked_jobs/{}", job_name), "")?;
        Ok(())
    }
    fn mark_need_cleanup(&self, job_name: &str) -> Result<(), Box<dyn Error>> {
        if config::Config::is_running_on_ci() {
            fs::create_dir_all("/tmp/deplo/need_cleanup_jobs")?;
            fs::write(format!("/tmp/deplo/need_cleanup_jobs/{}", job_name), "")?;
        } else {
            log::debug!("mark_need_cleanup: {}", job_name);
        }
        Ok(())
    }
    fn filter_workflows(
        &self, _trigger: Option<ci::WorkflowTrigger>
    ) -> Result<Vec<config::runtime::Workflow>, Box<dyn Error>> {
        log::warn!("TODO: implement filter_workflows for circleci");
        Ok(vec![])
    }
    fn run_job(&self, _job_config: &config::runtime::Workflow) -> Result<String, Box<dyn Error>> {
        log::warn!("TODO: implement run_job for circleci");
        Ok("".to_string())
    }
    fn check_job_finished(&self, _: &str) -> Result<Option<String>, Box<dyn Error>> {
        log::warn!("TODO: implement check_job_finished for circleci");
        Ok(None)
    }
    fn job_output(&self, _: &str, _: ci::OutputKind, _: &str) -> Result<Option<String>, Box<dyn Error>> {
        log::warn!("TODO: implement job_output for circleci");
        Ok(None)
    }
    fn set_job_output(&self, _: &str, _: ci::OutputKind, _: HashMap<&str, &str>) -> Result<(), Box<dyn Error>> {
        log::warn!("TODO: implement set_job_output for circleci");
        Ok(())
    }
    fn set_job_env(&self, _envs: HashMap<&str, &str>) -> Result<(), Box<dyn Error>> {
        log::warn!("TODO: implement set_job_env for circleci");
        Ok(())
    }
    fn process_env(&self) -> Result<HashMap<&str, String>, Box<dyn Error>> {
        let mut envs = hashmap!{
            "DEPLO_CI_TYPE" => "CircleCI".to_string(),
        };
        if config::Config::is_running_on_ci() {
            envs.insert(config::DEPLO_RUNNING_ON_CI_ENV_KEY, "true".to_string());
        }
        // get from env
        for (target, src) in hashmap!{
            "DEPLO_CI_ID" => "CIRCLE_WORKFLOW_ID",
            "DEPLO_CI_PULL_REQUEST_URL" => "CIRCLE_PULL_REQUEST",
            "DEPLO_CI_CURRENT_COMMIT_ID" => "CIRCLE_SHA1",
        } {
            match std::env::var(src) {
                Ok(v) => {
                    envs.insert(target, v);
                },
                Err(_) => {}
            };
        };
        Ok(envs)
    }
    fn generate_token(&self, _token_config: &ci::TokenConfig) -> Result<String, Box<dyn Error>> {
        return escalate!(Box::new(ci::CIError {
            cause: format!("TODO: support token generation")
        }));
    }
    fn job_env(&self) -> HashMap<String, config::Value> {
        hashmap!{}
    }
    fn list_secret_name(&self) -> Result<Vec<String>, Box<dyn Error>> {
        log::warn!("TODO: implement list_secret_name for circleci");
        Ok(vec![])   
    }
    fn set_secret(&self, key: &str, val: &str) -> Result<(), Box<dyn Error>> {
        let config = self.config.borrow();
        let token = match &config.ci.get(&self.account_name).expect(&format!("no ci config for {}", self.account_name)) {
            config::ci::Account::CircleCI { key, .. } => { key },
            _ => { 
                return escalate!(Box::new(ci::CIError {
                    cause: "should have circleci CI config but ghaction config provided".to_string()
                }));
            }
        };
        let json = format!("{{\"name\":\"{}\",\"value\":\"{}\"}}", key, val);
        let user_and_repo = config.modules.vcs().user_and_repo()?;
        let status = self.shell.exec(shell::args!(
            "curl", "-X", "POST", "-u", format!("{}:", token),
            format!(
                "https://circleci.com/api/v2/project/gh/{}/{}/envvar",
                user_and_repo.0, user_and_repo.1
            ),
            "-H", "Content-Type: application/json",
            "-H", "Accept: application/json",
            "-d", json, "-w", "%{http_code}", "-o", "/dev/null"
        ), shell::no_env(), shell::no_cwd(), &shell::capture())?.parse::<u32>()?;
        if status >= 200 && status < 300 {
            Ok(())
        } else {
            return escalate!(Box::new(ci::CIError {
                cause: format!("fail to set secret to CircleCI CI with status code:{}", status)
            }));
        }
    }
}
