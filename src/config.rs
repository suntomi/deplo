use std::fs;
use std::path;
use std::collections::{HashMap};

use log;
use simple_logger;
use serde::{Deserialize, Serialize};
use toml::de::{Error};
use dotenv::dotenv;

use crate::args;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CloudProviderConfig {
    GCP {
        key: String
    },
    AWS {
        key_id: String,
        secret_key: String
    },
    ALI {
        key: String
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CIConfig {
    Github {
        account: String,
        key: String
    },
    Circle {
        key: String
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StoreConfig {
    Apple {
        account: String,
        password: String
    },
    Google {
        key: String
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum VCSConfig {
    Github {
        email: String,
        account: String,
        key: String
    },
    Gitlab {
        email: String,
        account: String,
        key: String
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TerraformerConfig {
    TerraformGCP {
        backend_bucket: String,
        backend_bucket_prefix: String,    
        root_domain: String,
        project_id: String,
        region: String,
    }
}
#[derive(Serialize, Deserialize)]
pub struct CloudConfig {
    pub provider: CloudProviderConfig,
    pub terraformer: TerraformerConfig,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PlatformBuildConfig {
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
#[derive(Serialize, Deserialize)]
pub struct ClientConfig {
    pub org_name: String,
    pub app_name: String,
    pub app_code: String,
    pub app_id: String,
    pub client_project_path: String,
    pub artifact_path: String,
    pub version_config_path: String,

    pub unity_path: String,
    pub serial_code: String,
    pub account: String,
    pub password: String,

    pub platform_build_configs: Vec<PlatformBuildConfig>,

    pub stores: Vec<StoreConfig>,
}
#[derive(Serialize, Deserialize)]
pub struct CommonConfig {
    pub deplo_image: String,
    pub data_dir: String,
    pub no_confirm_for_prod_deploy: bool,
}
#[derive(Serialize, Deserialize)]
pub struct DeployConfig {
    pub pr: HashMap<String, String>,
    pub release: HashMap<String, String>,
}
#[derive(Default)]
pub struct CliConfig<'a> {
    pub verbosity: u64,
    pub dryrun: bool,
    pub debug: Vec<&'a str>,
}
#[derive(Serialize, Deserialize)]
pub struct Config<'a> {
    #[serde(skip)]
    pub cli: CliConfig<'a>,
    pub common: CommonConfig,
    pub cloud: CloudConfig,
    pub vcs: VCSConfig,
    pub ci: CIConfig,
    pub client: ClientConfig,
    pub deploy: DeployConfig
}

impl<'a> Config<'a> {
    // static factory methods 
    pub fn load(path: &str) -> Result<Config, Error> {
        let mut content = fs::read_to_string(path).unwrap();
        if envsubst::is_templated(&content) {
            content = envsubst::substitute(&content, &std::env::vars().collect()).unwrap();
        }
        return toml::from_str(&content);
    }
    pub fn create<A: args::Args>(args: &A) -> Result<Config, Error> {
        let verbosity = args.occurence_of("verbosity");
        simple_logger::init_with_level(match verbosity {
            0 => log::Level::Warn,
            1 => log::Level::Info,
            2 => log::Level::Debug,
            3 => log::Level::Trace,
            _ => log::Level::Warn
        }).unwrap();
        // load dotenv
        match args.value_of("dotenv") {
            Some(dotenv_path) => {
                dotenv::from_filename(dotenv_path).unwrap();
            },
            None => match dotenv() {
                Ok(_) => {},
                Err(err) => {
                    log::warn!(".env not present or cannot load by error [{:?}], this usually means:\n\
                                1. command will be run with incorrect parameter or\n\
                                2. secrets are directly written in deplo.toml\n\
                                please use .env to store secrets, or use -e flag to specify its path", 
                                err)
                } 
            },
        };
        //println!("DEPLO_CLIENT_IOS_TEAM_ID:{}", std::env::var("DEPLO_CLIENT_IOS_TEAM_ID").unwrap());    
        let mut c = Config::load(args.value_of("config").unwrap_or("./deplo.toml")).unwrap();
        c.cli = CliConfig {
            verbosity,
            dryrun: args.occurence_of("dryrun") > 0,
            debug: match args.values_of("debug") {
                Some(s) => s,
                None => vec!{}
            }
        };
        return Ok(c);
    }
    pub fn root_path(&self) -> &path::Path {
        return path::Path::new(&self.common.data_dir);
    }
    pub fn services_path(&self) -> path::PathBuf {
        return path::Path::new(&self.common.data_dir).join("services");
    }
}
