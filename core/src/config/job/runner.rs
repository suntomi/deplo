use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;

use maplit::hashmap;

use crate::config;
use crate::config::job;
use crate::shell;
use crate::util::{defer, merge_hashmap, rm};
use crate::vcs;

pub struct Runner<'a> {
    pub config: &'a config::Config,
    pub job: &'a config::job::Job
}

impl<'a> Runner<'a> {
    pub fn new(job: &'a config::job::Job, config: &'a config::Config) -> Self {
        Self { config, job }
    }
    fn adjust_commit_hash(&self, commit: &Option<&str>) -> Result<(), Box<dyn Error>> {
        let config = self.config;
        if let Some(ref c) = commit {
            let vcs = config.modules.vcs();
            if config::Config::is_running_on_ci() {
                let target = vcs.commit_hash(Some(c))?;
                let current = vcs.commit_hash(None)?;
                if target != current {
                    panic!("on CI, HEAD should already set to '{}' but '{}'", target, current);
                }
            } else {
                log::debug!("change commit hash to {}", c);
                vcs.checkout(c, Some(config::DEPLO_VCS_TEMPORARY_WORKSPACE_NAME))?;
            }
        }
        Ok(())
    }
    fn recover_branch(&self) -> Result<(), Box<dyn Error>> {
        let config = self.config;
        if !config::Config::is_running_on_ci() {
            let vcs = config.modules.vcs();
            let (ref_type, ref_path) = vcs.current_ref()?;
            if ref_type == vcs::RefType::Branch &&
                ref_path == config::DEPLO_VCS_TEMPORARY_WORKSPACE_NAME {
                log::debug!("back to previous branch");
                vcs.checkout_previous()?;
            }
        }
        Ok(())
    }
    pub fn create_steps(&self, command: &job::Command) -> (Vec<job::Step>, Option<String>) {   
        let job = self.job;
        match command {
            job::Command::Adhoc(ref c) => {
                (vec![job::Step{
                    name: None,
                    env: None,
                    command: job::StepCommand::Eval{
                        command: config::Value::new(c),
                        workdir: None,
                        shell: job.shell.clone()
                    }
                }], Some(c.to_string()))
            },
            job::Command::Job => if let Some(steps) = &job.steps {
                (steps.to_vec(), None)
            } else if let Some(ref c) = &job.command {
                (vec![job::Step{
                    name: None,
                    env: None,
                    command: job::StepCommand::Eval{
                        command: c.clone(),
                        workdir: None,
                        shell: job.shell.clone()
                    }
                }], Some(c.to_string()))
            } else {
                panic!("neither job.command nor job.steps specified");
            },
            job::Command::Shell => {
                let c = job.shell.as_ref().map_or_else(|| config::Value::new("bash"), |v| v.clone());
                (vec![job::Step{
                    name: None,
                    env: None,
                    command: job::StepCommand::Eval{
                        command: c.clone(),
                        workdir: None,
                        shell: job.shell.clone()
                    }
                }], Some(c.to_string()))
            }
        }
    }
    fn step_runner_command(
        job_name: &str, runtime_workflow_config: &config::runtime::Workflow, task_name: &Option<String>
    ) -> String {
        let payload = serde_json::to_string(runtime_workflow_config).expect(
            &format!("fail to get json string from runtime_workflow_config for {}", job_name)
        );
        match task_name {
            // TODO: able to specify steps for tasks
            Some(v) => format!("deplo job run-steps {} -p '{}' --task={}", job_name, payload, v),
            None => format!("deplo job run-steps {} -p '{}'", job_name, payload)
        }
    }
    pub fn run_steps(
        &self, shell: &impl shell::Shell, shell_settings: &shell::Settings,
        runtime_workflow_config: &config::runtime::Workflow,
        job: &config::job::Job, steps: &Vec<job::Step>
    ) -> Result<Option<String>, Box<dyn Error>> {
        let empty_envs = hashmap!{};
        let job_envs = job.env(self.config, runtime_workflow_config);
        for step in steps {
            match &step.command {
                job::StepCommand::Eval{shell: sh, command, workdir} => {
                    shell.eval(
                        command.as_str(),
                        &sh.as_ref().map_or_else(|| job.shell.as_ref().map(|v| v.resolve()), |v| Some(v.as_str().to_string())),
                        shell::mctoa(merge_hashmap(&job_envs, step.env.as_ref().map_or_else(|| &empty_envs, |v| v))),
                        &workdir.as_ref().map_or_else(|| job.workdir.as_ref(), |v| Some(&v)),
                        shell_settings
                    )?;
                },
                job::StepCommand::Exec{exec, workdir} => {
                    shell.exec(exec.iter().map(|v| shell::arg!(v)),
                        shell::mctoa(merge_hashmap(&job_envs, step.env.as_ref().map_or_else(|| &empty_envs, |v| v))),
                        &workdir.as_ref().map_or_else(|| job.workdir.as_ref(), |v| Some(&v)),
                        shell_settings
                    )?;
                },
                job::StepCommand::Module(c) => {
                    c.value(|v| {
                        self.config.modules.step(&v.uses).run(
                            shell_settings, &job_envs, &v.with
                        )
                    })?;
                }
            }
        };
        Ok(None)
    }
    pub fn run(
        &self, shell: &impl shell::Shell, runtime_workflow_config: &config::runtime::Workflow
    ) -> Result<Option<String>, Box<dyn Error>> {
        let config = self.config;
        let job = self.job;
        match config.runtime.debug_options.get("dryrun") {
            Some(_) => {
                log::warn!("skip running job '{}' by dryrun", job.name);
                return Ok(None);
            },
            None => {}
        }
        let exec = &runtime_workflow_config.exec;
        // apply exec settings to current workspace.
        let command = runtime_workflow_config.command();
        // silent
        let shell_settings = &mut match command {
            job::Command::Shell => shell::interactive(),
            _ => if exec.silent {
                shell::silent()
            } else {
                shell::no_capture()
            }
        };
        let (steps, cmd_for_container) = self.create_steps(&command);
        if exec.remote {
            log::debug!(
                "force running job '{}' on remote with steps {} at {}",
                job.name, job::StepsDumper{steps: &steps}, exec.revision.as_ref().unwrap_or(&"".to_string())
            );
            let ci = job.ci(&config);
            return Ok(Some(ci.run_job(&runtime_workflow_config)?));
        }
        // adjust revision with command line argument
        self.adjust_commit_hash(&exec.revision.as_ref().map(|v| v.as_str()))?;
        defer!{self.recover_branch().unwrap();};
        match job.runner {
            job::Runner::Machine{os, ref local_fallback, ..} => {
                let current_os = shell.detect_os()?;
                if os == current_os {
                    if let Some(p) = config.setup_deplo_cli(os, shell)? {
                        let parent = p.parent().expect(&format!("path should not be root {}", p.display()));
                        shell_settings.paths(vec![parent.to_string_lossy().to_string()]);
                    };
                    // run command directly here, add path to locally downloaded cli.
                    self.run_steps(shell, &shell_settings, runtime_workflow_config, job, &steps)?;
                    self.post_run(runtime_workflow_config)?;
                } else {
                    log::debug!("runner os '{}' is different from current os '{}'", os, current_os);
                    match local_fallback {
                        Some(f) => {
                            let (image, sh) = match &f.source {
                                job::ContainerImageSource::ImageUrl{ image } => (image.clone(), &f.shell),
                                job::ContainerImageSource::DockerFile{ path, repo_name, args: build_args } => {
                                    let local_image = match repo_name.as_ref() {
                                        Some(n) => format!("{}:{}", n, job.name),
                                        None => format!("{}-deplo-local-fallback:{}", config.project_name, job.name)
                                    };
                                    log::info!("generate fallback docker image {} from {}", local_image, path);
                                    let path_string = path.resolve();
                                    let path = Path::new(&path_string);
                                    let mut args = shell::args![
                                        "docker", "build",
                                        "-t", local_image.as_str(),
                                        "-f", path.file_name().unwrap().to_str().unwrap()
                                    ];
                                    for (k, v) in build_args.iter().flatten() {
                                        args.push(shell::arg!("--build-arg"));
                                        args.push(shell::arg!(shell::fmtargs!("{}={}", k, v)));
                                    }
                                    args.push(shell::arg!("."));
                                    shell.exec(
                                        args, shell::no_env(),
                                        &Some(path.parent().unwrap().to_string_lossy().to_string()),
                                        &shell::capture_inherit()
                                    )?;
                                    (config::Value::new(&local_image), &f.shell)
                                },
                            };
                            let path = &config.setup_deplo_cli(os, shell)?.expect("local fallback only invoked on local machine");
                            let path_target = config::Value::new("/usr/local/bin/deplo");
                            let mut mounts = shell::ContainerMounts::new(config, job);
                            shell.eval_on_container(
                                image.as_str(),
                                // if main_command is none, we need to run steps in single container.
                                // so we execute `deplo job $job_name steps` to run steps of $job_name.
                                &cmd_for_container.map_or_else(|| Self::step_runner_command(
                                    &job.name, runtime_workflow_config, &None
                                ), |v| match sh {
                                    // if command is shell command and local fallback has dedicate shell setting,
                                    // override shell setting with local_fallback's one.
                                    Some(sh) => match command {
                                        job::Command::Shell => sh.resolve(),
                                        _ => v.to_string(),
                                    },
                                    None => v.to_string()
                                }),
                                &sh.as_ref().map(|v| v.resolve()), shell::mctoa(job.env(&config, runtime_workflow_config)),
                                &job.workdir, mounts.bind(hashmap!{
                                    path.as_os_str() => &path_target
                                }), &shell_settings
                            )?;
                            self.post_run(runtime_workflow_config)?;
                            return Ok(None);
                        },
                        None => ()
                    };
                    // runner os is not linux and not same as current os, and no fallback container specified.
                    // need to run in CI.
                    log::debug!(
                        "running job '{}' on remote with steps {} at {}: its target os={} which cannot fallback to local execution",
                        job.name, job::StepsDumper{steps: &steps}, exec.revision.as_ref().unwrap_or(&"".to_string()), os
                    );
                    let ci = job.ci(&config);
                    return Ok(Some(ci.run_job(&runtime_workflow_config)?));
    
                }
            },
            job::Runner::Container{ ref image, .. } => {
                if config::Config::is_running_on_ci() {
                    // already run inside container `image`, run command directly here
                    // no need to setup_deplo_cli because CI should already setup it
                    self.run_steps(shell, &shell_settings, runtime_workflow_config, job, &steps)?;
                } else {
                    let path = &config.setup_deplo_cli(job::RunnerOS::Linux, shell)?.expect("path should return because not running on CI");
                    let path_target = config::Value::new("/usr/local/bin/deplo");
                    let mut mounts = shell::ContainerMounts::new(config, job);
                    // running on host. run command in container `image` with docker
                    shell.eval_on_container(
                        image.as_str(),
                        // if main_command is none, we need to run steps in single container. 
                        // so we execute `deplo job $job_name steps` to run steps of $job_name.
                        &cmd_for_container.map_or_else(|| Self::step_runner_command(
                            &job.name, runtime_workflow_config, &None
                        ), |v| v.to_string()),
                        &job.shell.as_ref().map(|v| v.resolve()), shell::mctoa(job.env(&config, runtime_workflow_config)),
                        &job.workdir, mounts.bind(hashmap!{
                            path.as_os_str() => &path_target
                        }), &shell_settings
                    )?;
                }
                self.post_run(runtime_workflow_config)?;
            }
        }
        Ok(None)
    }
    pub fn post_run(&self, runtime_workflow_config: &config::runtime::Workflow) -> Result<(), Box<dyn Error>> {
        let job = self.job;
        let config = self.config;
        let job_name = &job.name;
        let mut system_job_outputs = hashmap!{};
        match job.commit_setting_from_config(&config, runtime_workflow_config) {
            Some(commit) => {
                let vcs = config.modules.vcs();
                match vcs.current_ref()? {
                    (vcs::RefType::Branch|vcs::RefType::Pull, _) => {
                        let branch_name = format!(
                            "deplo-auto-commits-{}-tmp-{}", 
                            crate::util::env::var_or_die("DEPLO_CI_ID"),
                            job_name
                        );
                        if vcs.push_diff(
                            // basically the branch_name does not exists in remote,
                            // we need to add refs/heads to create it automatically
                            &format!("refs/heads/{}", branch_name), 
                            &commit.generate_commit_log(job_name, &job),
                            &commit.files.iter().map(|v| v.as_str()).collect::<Vec<&str>>(),
                            &hashmap!{}
                        )? {
                            system_job_outputs.insert(config::job::DEPLO_SYSTEM_OUTPUT_COMMIT_BRANCH_NAME, branch_name);
                        }
                    },
                    (ty, b) => {
                        log::warn!("current ref is not a branch ({}/{}), skip auto commit", ty, b);
                    }
                }
            },
            None => { 
                log::debug!("no commit settings for job '{}' in workflow '{}'", job.name, runtime_workflow_config.name);
            }
        };
        let ci = job.ci(&config);
        if system_job_outputs.len() > 0 {
            log::debug!("set system job outputs: {:?}", system_job_outputs);
            ci.set_job_output(
                job_name, crate::ci::OutputKind::System, 
                system_job_outputs.iter().map(|(k,v)| (*k, v.as_str())).collect()
            )?;
            ci.mark_need_cleanup(job_name)?;
        }
        match fs::read(Path::new(config::job::DEPLO_JOB_OUTPUT_TEMPORARY_FILE)) {
            Ok(b) => {
                let outputs = serde_json::from_slice::<HashMap<&str, &str>>(&b)?;
                log::debug!("set user job outputs: {:?}", outputs);
                ci.set_job_output(job_name, crate::ci::OutputKind::User, outputs)?;
                rm(config::job::DEPLO_JOB_OUTPUT_TEMPORARY_FILE);
            },
            Err(_) => {}
        }
        Ok(())
    }    
}