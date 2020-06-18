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
use crate::util::{escalate, envsubst};

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
pub enum ContainerDeployTarget {
    Instance,
    Kubernetes,
    Serverless,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum DistributionConfig {
    Apple {
        account: String,
        password: String
    },
    Google {
        key: String
    },
    Storage {
        bucket_name: String
    }
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum UnityPlatformBuildConfig {
    Android {
        keystore_password: String,
        keyalias_name: String,
        keyalias_password: String,
        keystore_path: String,
        use_expansion_file: bool,            
    },
    IOS {
        team_id: String,
        numeric_team_id: String,
        signing_password: String,
        signing_plist_path: String,
        signing_p12_path: String,
        singing_provision_path: String,
    }
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum Builder {
    Unity {
        unity_version: String,
        serial_code: String,
        account: String,
        password: String,
        platform: UnityPlatformBuildConfig,
    },
    CreateReactApp {
    }
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
enum Step {
    Script {
        code: String,
        runner: Option<String>,
        workdir: Option<String>,
        env: Option<HashMap<String, String>>,
    },
    Container {
        image: String,
        target: ContainerDeployTarget,
        port: u32,
        extra_ports: Option<HashMap<String, u32>>,
        env: Option<HashMap<String, String>>,
        options: Option<HashMap<String, String>>,
    },
    Storage {
        // source file glob pattern => target storage path
        copymap: HashMap<String, cloud::DeployStorageOption>,
    },
    Build {
        org_name: String,
        app_name: String,
        app_id: String,
        project_path: String,
        artifact_path: Option<String>,
        builder: Builder,
    },
    Distribution {
        config: DistributionConfig,
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
                    Ok(_) => shell.exec(
                        &vec!(runner_command, code), 
                        env.as_ref().unwrap_or(&hashmap!{}), false
                    ),
                    Err(_) => {
                        shell.eval(
                            &format!("echo \'{}\' | {}", code, runner_command), 
                            env.as_ref().unwrap_or(&hashmap!{}), false
                        )
                    }
                };
                shell.set_cwd::<String>(None)?;
                match r {
                    Ok(_) => Ok(()),
                    Err(err) => escalate!(Box::new(err))
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
                return cloud.deploy_container(
                    plan, &target, &pushed_image_tag, &ports, 
                    env.as_ref().unwrap_or(&hashmap!{}), 
                    options.as_ref().unwrap_or(&hashmap!{})
                );
            },
            Self::Storage { copymap } => {
                let config = plan.config;
                let cloud = config.cloud_service()?;
                return cloud.deploy_storage(cloud::StorageKind::Service{plan}, &copymap);
            },
            _ => {
                return Ok(())
            }
        }
    }
}
#[derive(Serialize, Deserialize, Debug)]
pub struct Sequence {
    steps: Vec<Step>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PlanData {
    pr: Sequence,
    deploy: Sequence
}

pub struct Plan<'a> {
    pub service: String,
    config: &'a config::Config<'a>,
    data: PlanData
}
impl<'a> Plan<'a> {
    fn make_unity_build_step(platform_build_config: UnityPlatformBuildConfig) -> Step {
        Self::make_build_step(Builder::Unity {
            unity_version: "${DEPLO_BUILD_UNITY_VERSION}".to_string(),
            serial_code: "${DEPLO_BUILD_UNITY_SERIAL_CODE}".to_string(),
            account: "${DEPLO_BUILD_UNITY_ACCOUNT_EMAIL}".to_string(),
            password: "${DEPLO_BUILD_UNITY_ACCOUNT_PASSWORD}".to_string(),
            platform: platform_build_config
        })
    }
    fn make_build_step(builder: Builder) -> Step {
        return Step::Build {
            org_name: "${DEPLO_ORG_NAME}".to_string(),
            app_name: "${DEPLO_APP_NAME}".to_string(),
            app_id: "${DEPLO_APP_ID}".to_string(),
            project_path: "./client".to_string(),
            artifact_path: None,
            builder
        }
    }
    pub fn create(
        config: &'a config::Config, 
        service: &str, kind: &str
    ) -> Result<Plan<'a>, Box<dyn Error>> {
        Ok(Plan::<'a> {
            service: service.to_string(),
            config,
            data: PlanData {
                pr: Sequence {
                    steps: vec!()
                },
                deploy: Sequence {
                    steps: match kind {
                        "container" => vec!(Step::Script {
                            code: "#!/bin/bash\n\
                                echo 'build conteiner'\n\
                                docker build -t your/image rsc/docker/base\n\
                                ".to_string(),
                            runner: None,
                            workdir: None,
                            env: Some(hashmap!{}),
                        }, Step::Container {
                            image: "your/image".to_string(),
                            target: ContainerDeployTarget::Instance,
                            port: 80,
                            extra_ports: None,
                            env: Some(hashmap!{}),
                            options: Some(hashmap!{}),
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
                        "unity_ios" => vec!(
                            Self::make_unity_build_step(UnityPlatformBuildConfig::IOS{
                                team_id: "${DEPLO_BUILD_UNITY_IOS_TEAM_ID}".to_string(),
                                numeric_team_id: "${DEPLO_BUILD_UNITY_IOS_NUMERIC_TEAM_ID}".to_string(),
                                signing_password: "${DEPLO_BUILD_UNITY_IOS_P12_SIGNING_PASSWORD}".to_string(),
                                signing_plist_path: "${DEPLO_BUILD_UNITY_IOS_SIGNING_FILES_PATH}/distribution.plist".to_string(),
                                signing_p12_path: "${DEPLO_BUILD_UNITY_IOS_SIGNING_FILES_PATH}/distribution.p12".to_string(),
                                singing_provision_path: "${DEPLO_BUILD_UNITY_IOS_SIGNING_FILES_PATH}/appstore.mobileprovision".to_string(),
                            }),
                            Step::Distribution {
                                config: DistributionConfig::Apple {
                                    account: "${DEPLO_DISTRIBUTION_APPLE_ACCOUNT}".to_string(),
                                    password: "${DEPLO_DISTRIBUTION_APPLE_PASSWORD}".to_string()
                                }
                            }
                        ),
                        "unity_android" => vec!(
                            Self::make_unity_build_step(UnityPlatformBuildConfig::Android{
                                keystore_password: "${DEPLO_BUILD_UNITY_ANDROID_KEYSTORE_PASSWORD}".to_string(),
                                keyalias_name: "${DEPLO_BUILD_UNITY_ANDROID_KEYSTORE_NAME}".to_string(),
                                keyalias_password: "${DEPLO_BUILD_UNITY_ANDROID_KEYALIAS_PASSWORD}".to_string(),
                                keystore_path: "${DEPLO_BUILD_UNITY_ANDROID_KEYSTORE_PATH}".to_string(),
                                use_expansion_file: false
                            }),
                            Step::Distribution {
                                config: DistributionConfig::Google {
                                    key: "${DEPLO_DISTRIBUTION_GOOGLE_ACCESS_KEY}".to_string()
                                }
                            }
                        ),
                        "cra" => vec!(
                            Self::make_build_step(Builder::CreateReactApp{}),
                            Step::Distribution {
                                config: DistributionConfig::Storage {
                                    bucket_name: "${DEPLO_DISTRIBUTION_STORAGE_BUCKET_NAME}".to_string()
                                }
                            }
                        ),
                        _ => return escalate!(Box::new(DeployError {
                            cause: format!("invalid deploy type: {:?}", kind)
                        }))
                    }
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
            None => return escalate!(Box::new(DeployError {
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
                Err(e) => return escalate!(Box::new(e))
            }
        }
        escalate!(Box::new(DeployError {
            cause: format!("plan contains endpoint:{} does not found", endpoint)
        }))
    }
    pub fn save(&self) -> Result<(), Box<dyn Error>> {
        let pathbuf = self.config.services_path().join(format!("{}.toml", self.service));
        match pathbuf.to_str() {
            Some(path) => return self.save_plandata(path),
            None => return escalate!(Box::new(DeployError {
                cause: format!("invalid path string: {:?}", pathbuf)
            }))
        }
    }
    pub fn exec<S: shell::Shell<'a>>(&self, pr: bool) -> Result<(), Box<dyn Error>> {
        for step in if pr { &self.data.pr.steps } else { &self.data.deploy.steps } {
            step.exec::<S>(self)?
        }
        Ok(())
    }
    pub fn has_deployment_of(&self, kind: &str) -> Result<bool, Box<dyn Error>> {
        match kind {
            "service" => {},
            "storage" => {},
            "distribution" => {},
            _ => return Err(Box::new(DeployError {
                cause: format!("invalid deployment kind {}", kind)
            }))
        }
        for step in &self.data.deploy.steps {
            match step {
                Step::Container { target:_, image:_, port:_, extra_ports:_, env:_, options:_ } => {
                    return Ok(kind == "service" || kind == "any")
                },
                Step::Storage { copymap:_ } => {
                    return Ok(kind == "storage" || kind == "any")
                },
                Step::Distribution { config:_ } => {
                    return Ok(kind == "distribution" || kind == "any")
                },
                _ => {}
            }
        }
        return escalate!(Box::new(DeployError {
            cause: format!("no container/storage are deployed in {}.toml", self.service)
        }))
    }
    pub fn ports(&self) -> Result<Option<HashMap<String, u32>>, Box<dyn Error>> {
        for step in &self.data.deploy.steps {
            match step {
                Step::Container { target:_, image:_, port, extra_ports, env:_, options:_ } => {
                    let mut ports = extra_ports.clone().unwrap_or(hashmap!{});
                    ports.entry("".to_string()).or_insert(*port);
                    return Ok(Some(ports))
                },
                Step::Storage { copymap:_ } => return Ok(None),
                Step::Distribution { config:_ } => return Ok(None),
                _ => {}
            }
        }
        return escalate!(Box::new(DeployError {
            cause: format!("either storage/container deployment should exist in {}.toml", self.service)
        }))
    }

    fn verify(&self) -> Result<(), Box<dyn Error>> {
        let err = Box::new(DeployError {
            cause: format!(
                "only a storage/container/store deployment can exist in release steps of {}.toml", 
                self.service
            )
        });
        let mut deployment_found = false;
        for steps in vec!(&self.data.deploy.steps, &self.data.pr.steps) {
            let pr = steps as *const _ == &self.data.pr.steps as *const _;
            for step in steps {
                match step {
                    Step::Container { target:_, image:_, port:_, extra_ports:_, env:_, options:_ } => {
                        if pr { return Err(err) }
                        if deployment_found { return Err(err) }
                        deployment_found = true;
                    },
                    Step::Storage { copymap:_ } => {
                        if pr { return Err(err) }
                        if deployment_found { return Err(err) }
                        deployment_found = true;
                    },
                    Step::Distribution { config:_ } => {
                        if pr { return Err(err) }
                        if deployment_found { return Err(err) }
                        deployment_found = true;
                    },
                    _ => {}
                }
            }
        }
        Ok(())
    }
    fn load_plandata(path_or_text: &str) -> Result<PlanData, Box<dyn Error>> {
        let data = match fs::read_to_string(path_or_text) {
            Ok(text) => toml::from_str::<PlanData>(&envsubst(&text))?,
            Err(_) => match toml::from_str::<PlanData>(&envsubst(&path_or_text)) {
                Ok(p) => p,
                Err(err) => return escalate!(Box::new(DeployError {
                    cause: format!(
                        "load_plandata: cannot load plan data from {} by {}", path_or_text, err
                    )
                }))
            }
        };
        Ok(data)
    }
    fn save_plandata(&self, path: &str) -> Result<(), Box<dyn Error>> {
        let mut as_text = String::new();
        let mut ser = toml::Serializer::new(&mut as_text);
        ser.pretty_string_literal(false);
        self.data.serialize(&mut ser)?;
        match fs::write(path, &as_text) {
            Ok(_) => Ok(()),
            Err(err) => escalate!(Box::new(err))
        }
    }
}
