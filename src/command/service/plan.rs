use std::fs;
use std::fmt;
use std::error::Error;
use std::collections::{HashMap};

use serde::{Deserialize, Serialize};
use maplit::hashmap;

use crate::config;
use crate::shell;

#[derive(Debug)]
pub struct DeployError {
    pub cause: String
}
impl fmt::Display for DeployError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl Error for DeployError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct ImageConfig {
    id: String,
    build: String
}

#[derive(Serialize, Deserialize, Debug)]
pub enum DeployTarget {
    Instance,
    Kubernetes,
    Serverless,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
enum Step {
    Script {
        code: String,
        runner: Option<String>,
        workdir: Option<String>,
        env: HashMap<String, String>,
    },
    Container {
        image: String,
        target: DeployTarget,
        ports: Vec<u32>,
        env: HashMap<String, String>,
        options: HashMap<String, String>,
    },
    Storage {
        // source file glob pattern => target storage path
        copymap: HashMap<String, String>,
    }
}
impl Step {
    pub fn exec<'a, S: shell::Shell<'a>>(&self, plan: &Plan<'a>) -> Result<(), Box<dyn Error>> {
        match self {
            Self::Script { code, runner, env, workdir } => {
                let default = "bash".to_string();
                let runner_command = runner.as_ref().unwrap_or(&default);
                let mut shell = S::new(plan.config);
                shell.set_cwd(workdir.as_ref())?;
                let r = match fs::metadata(code) {
                    Ok(_) => shell.exec(&vec!(runner_command, code), &env, false),
                    Err(_) => {
                        shell.eval(&format!("echo \'{}\' | {}", code, runner_command), &env, false)
                    }
                };
                shell.set_cwd::<String>(None)?;
                match r {
                    Ok(_) => Ok(()),
                    Err(err) => Err(Box::new(err))
                }
            },
            Self::Container { target, image, ports, env, options } => {
                let config = plan.config;
                let cloud = config.cloud_service()?;
                // deploy image to cloud container registry
                let pushed_image_tag = cloud.push_container_image(&image, 
                    &format!("{}:{}", 
                        &config.canonical_name(&plan.service), 
                        config.next_service_endpoint_version(&plan.service)?)
                )?;
                // deploy to autoscaling group or serverless platform
                return cloud.deploy_container(plan, &target, &pushed_image_tag, ports, env, options);
            },
            Self::Storage { copymap } => {
                let config = plan.config;
                let cloud = config.cloud_service()?;
                return cloud.deploy_storage(&copymap)
            },
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PlanData {
    steps: Vec<Step>,
}

pub struct Plan<'a> {
    pub service: String,
    config: &'a config::Config<'a>,
    data: PlanData
}
impl<'a> Plan<'a> {
    pub fn create(
        config: &'a config::Config, 
        service: &str, kind: &str
    ) -> Result<Plan<'a>, Box<dyn Error>> {
        Ok(Plan::<'a> {
            service: service.to_string(),
            config,
            data: PlanData {
                steps: match kind {
                    "container" => vec!(Step::Script {
                        code: "#!/bin/bash\n\
                               echo 'build conteiner'\n\
                               docker build -t your/image rsc/docker/base\n\
                              ".to_string(),
                        runner: None,
                        workdir: None,
                        env: hashmap!{},
                    }, Step::Container {
                        image: "your/image".to_string(),
                        target: DeployTarget::Instance,
                        ports: vec!(80),
                        env: hashmap!{},
                        options: hashmap!{},
                    }), 
                    "storage" => vec!(Step::Storage {
                        copymap: hashmap! {
                            "source_dir/copyfiles/*.".to_string() => 
                            "target_bucket/folder/subfolder".to_string()
                        }
                    }),
                    "script" => vec!(Step::Script {
                        code: "#!/bin/bash\n\
                               build_image.sh your/image\n\
                               deploy_as_serverless.sh your/image your_autoscaling_group\n\
                              ".to_string(),
                        runner: Some("bash".to_string()),
                        workdir: None,
                        env: hashmap!{},
                    }),
                    _ => return Err(Box::new(DeployError {
                        cause: format!("invalid deploy type: {:?}", kind)
                    }))
                }
            }
        })
    }
    pub fn load(
        config: &'a config::Config, 
        service: &str
    ) -> Result<Plan<'a>, Box<dyn Error>> {
        let pathbuf = config.services_path().join(format!("{}.toml", service));
        match pathbuf.to_str() {
            Some(path) => return Ok(Plan::<'a> {
                service: service.to_string(),
                config,
                data: Self::load_plandata(path)?
            }),
            None => return Err(Box::new(DeployError {
                cause: format!("invalid path string: {:?}", pathbuf)
            }))
        }
    }
    pub fn save(&self) -> Result<(), Box<dyn Error>> {
        let pathbuf = self.config.services_path().join(format!("{}.toml", self.service));
        match pathbuf.to_str() {
            Some(path) => return self.save_plandata(path),
            None => return Err(Box::new(DeployError {
                cause: format!("invalid path string: {:?}", pathbuf)
            }))
        }
    }
    pub fn exec<S: shell::Shell<'a>>(&self) -> Result<(), Box<dyn Error>> {
        for step in &self.data.steps {
            step.exec::<S>(self)?
        }
        Ok(())
    }

    fn load_plandata(path_or_text: &str) -> Result<PlanData, Box<dyn Error>> {
        let data = match fs::read_to_string(path_or_text) {
            Ok(text) => toml::from_str::<PlanData>(&text)?,
            Err(_) => toml::from_str::<PlanData>(&path_or_text)?
        };
        Ok(data)
    }
    fn save_plandata(&self, path: &str) -> Result<(), Box<dyn Error>> {
        let as_text = toml::to_string_pretty(&self.data)?;
        match fs::write(path, &as_text) {
            Ok(_) => Ok(()),
            Err(err) => Err(Box::new(err))
        }
    }
}
