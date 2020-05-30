use std::fs;
use std::fmt;
use std::path;
use std::error::Error;
use std::collections::{HashMap};

use serde::{Deserialize, Serialize};
use maplit::hashmap;
use glob::glob;

use crate::config;
use crate::shell;
use crate::cloud;

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
        port: u32,
        extra_ports: Option<HashMap<String, u32>>,
        env: HashMap<String, String>,
        options: HashMap<String, String>,
    },
    Storage {
        // source file glob pattern => target storage path
        copymap: HashMap<String, cloud::DeployStorageOption>,
    },
    Store {
        kind: config::StoreKind,
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
            Self::Container { target, image, port:_, extra_ports:_, env, options } => {
                let config = plan.config;
                let cloud = config.cloud_service()?;
                // deploy image to cloud container registry
                let pushed_image_tag = cloud.push_container_image(&image, 
                    &format!("{}:{}", 
                        &config.canonical_name(&plan.service), 
                        config.next_service_endpoint_version(&plan.service)?)
                )?;
                // deploy to autoscaling group or serverless platform
                let ports = plan.ports()?.expect("container deployment should have at least an exposed port");
                return cloud.deploy_container(plan, &target, &pushed_image_tag, &ports, env, options);
            },
            Self::Storage { copymap } => {
                let config = plan.config;
                let cloud = config.cloud_service()?;
                return cloud.deploy_storage(cloud::StorageKind::Service{plan}, &copymap);
            },
            Self::Store { kind:_ } => {
                return Ok(())
            }
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
                        port: 80,
                        extra_ports: None,
                        env: hashmap!{},
                        options: hashmap!{},
                    }), 
                    "storage" => vec!(Step::Storage {
                        copymap: hashmap! {
                            "source_dir/copyfiles/*.".to_string() => 
                            cloud::DeployStorageOption {
                                destination: "target_bucket/folder/subfolder".to_string(),
                                permission: None,
                                max_age: None,
                                excludes: None
                            }
                        },
                    }),
                    "store" => vec!(Step::Store {
                        kind: config::StoreKind::AppStore,
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
        let path = config.services_path().join(format!("{}.toml", service));
        Plan::<'a>::load_by_path(config, &path)
    }
    pub fn load_by_path(
        config: &'a config::Config, path: &path::PathBuf
    ) -> Result<Plan<'a>, Box<dyn Error>> {
        let service = path.file_stem().unwrap();
        match path.to_str() {
            Some(path) => {
                let plan = Plan::<'a> {
                    service: service.to_string_lossy().to_string(),
                    config,
                    data: Self::load_plandata(path)?
                };
                plan.verify()?;
                return Ok(plan)
            }
            None => return Err(Box::new(DeployError {
                cause: format!("invalid path string: {:?}", path)
            }))
        }
    }
    pub fn find_by_endpoint(
        config: &'a config::Config, endpoint: &str
    ) -> Result<Plan<'a>, Box<dyn Error>> {
        for entry in glob(&config.services_path().join("*.toml").to_string_lossy())? {
            match entry {
                Ok(path) => {
                    let plan = Plan::<'a>::load_by_path(config, &path)?;
                    match plan.ports()? {
                        Some(ports) => match ports.get(endpoint) {
                            Some(_) => return Ok(plan),
                            None => if endpoint == plan.service { return Ok(plan) }
                        },
                        None => {}
                    }
                },
                Err(e) => return Err(Box::new(e))
            }
        }
        Err(Box::new(DeployError {
            cause: format!("plan contains endpoint:{} does not found", endpoint)
        }))
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
    pub fn has_store_deployment(&self) -> Result<bool, Box<dyn Error>> {
        for step in &self.data.steps {
            match step {
                Step::Script { code:_, runner:_, env:_, workdir:_ } => {},
                Step::Container { target:_, image:_, port:_, extra_ports:_, env:_, options:_ } => {
                    return Ok(false)
                },
                Step::Storage { copymap:_ } => {
                    return Ok(false)
                },
                Step::Store { kind:_ } => {
                    return Ok(true)
                }
            }
        }
        return Err(Box::new(DeployError {
            cause: format!("no container/storage are deployed in {}.toml", self.service)
        }))
    }
    pub fn has_bluegreen_deployment(&self) -> Result<bool, Box<dyn Error>> {
        for step in &self.data.steps {
            match step {
                Step::Script { code:_, runner:_, env:_, workdir:_ } => {},
                Step::Container { target:_, image:_, port:_, extra_ports:_, env:_, options:_ } => {
                    return Ok(true)
                },
                Step::Storage { copymap:_ } => {
                    return Ok(false)
                },
                Step::Store { kind:_ } => {
                    return Ok(false)
                }
            }
        }
        return Err(Box::new(DeployError {
            cause: format!("no container/storage are deployed in {}.toml", self.service)
        }))
    }
    pub fn ports(&self) -> Result<Option<HashMap<String, u32>>, Box<dyn Error>> {
        for step in &self.data.steps {
            match step {
                Step::Script { code:_, runner:_, env:_, workdir:_ } => {},
                Step::Container { target:_, image:_, port, extra_ports, env:_, options:_ } => {
                    let mut ports = extra_ports.clone().unwrap_or(hashmap!{});
                    ports.entry("".to_string()).or_insert(*port);
                    return Ok(Some(ports))
                },
                Step::Storage { copymap:_ } => {
                    return Ok(None)
                },
                Step::Store { kind:_ } => {
                    return Ok(None)
                }
            }
        }
        return Err(Box::new(DeployError {
            cause: format!("either storage/container deployment should exist in {}.toml", self.service)
        }))
    }

    fn verify(&self) -> Result<(), Box<dyn Error>> {
        let err = Box::new(DeployError {
            cause: format!(
                "only one storage/container/store deployment can exist in {}.toml", 
                self.service
            )
        });
        let mut deployment_found = false;
        for step in &self.data.steps {
            match step {
                Step::Script { code:_, runner:_, env:_, workdir:_ } => {},
                Step::Container { target:_, image:_, port:_, extra_ports:_, env:_, options:_ } => {
                    if deployment_found { return Err(err) }
                    deployment_found = true;
                },
                Step::Storage { copymap:_ } => {
                    if deployment_found { return Err(err) }
                    deployment_found = true;
                },
                Step::Store { kind:_ } => {
                    if deployment_found { return Err(err) }
                    deployment_found = true;
                }
            }
        }
        Ok(())
    }
    fn load_plandata(path_or_text: &str) -> Result<PlanData, Box<dyn Error>> {
        let data = match fs::read_to_string(path_or_text) {
            Ok(text) => toml::from_str::<PlanData>(&text)?,
            Err(_) => match toml::from_str::<PlanData>(&path_or_text) {
                Ok(p) => p,
                Err(err) => return Err(Box::new(DeployError {
                    cause: format!(
                        "load_plandata: cannot load plan data from {} by {}", path_or_text, err
                    )
                }))
            }
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
