use std::fs;
use std::fmt;
use std::path;
use std::error::Error;
use std::collections::{HashMap};

use log;
use simple_logger;
use serde::{Deserialize, Serialize};
use dotenv::dotenv;

use crate::args;
use crate::vcs;
use crate::cloud;
use crate::endpoints;

#[derive(Debug)]
pub struct ConfigError {
    pub cause: String
}
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

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
impl fmt::Display for CloudProviderConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GCP{ key:_ } => write!(f, "gcp"),
            Self::AWS{ key_id:_, secret_key:_ } => write!(f, "aws"),
            Self::ALI{ key:_ } => write!(f, "ali"),
        }
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
impl fmt::Display for VCSConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Github{ email:_, account:_, key:_ } => write!(f, "github"),
            Self::Gitlab{ email:_, account:_, key:_ } => write!(f, "gitlab"),
        }
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
impl TerraformerConfig {
    pub fn project_id(&self) -> &str {
        match self {
            Self::TerraformGCP{ 
                backend_bucket: _,
                backend_bucket_prefix: _,
                root_domain: _,
                project_id,
                region: _
            } => &project_id
        }
    }
    pub fn root_domain(&self) -> &str {
        match self {
            Self::TerraformGCP{ 
                backend_bucket: _,
                backend_bucket_prefix: _,
                root_domain,
                project_id: _,
                region: _
            } => &root_domain
        }
    }
    pub fn region(&self) -> &str {
        match self {
            Self::TerraformGCP{ 
                backend_bucket: _,
                backend_bucket_prefix: _,
                root_domain: _,
                project_id: _,
                region
            } => &region
        }
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
    pub release_targets: HashMap<String, String>,
}
#[derive(Serialize, Deserialize)]
pub struct DeployConfig {
    pub pr: HashMap<String, String>,
    pub release: HashMap<String, String>,
}
#[derive(Default)]
pub struct RuntimeConfig<'a> {
    pub verbosity: u64,
    pub dryrun: bool,
    pub debug: Vec<&'a str>,
    pub release_target: Option<String>,
}
#[derive(Serialize, Deserialize)]
pub struct Config<'a> {
    #[serde(skip)]
    pub runtime: RuntimeConfig<'a>,
    pub common: CommonConfig,
    pub cloud: CloudConfig,
    pub vcs: VCSConfig,
    pub ci: CIConfig,
    pub client: ClientConfig,
    pub deploy: DeployConfig
}

impl<'a> Config<'a> {
    // static factory methods 
    pub fn load(path: &str) -> Result<Config, Box<dyn Error>> {
        let mut content = fs::read_to_string(path).unwrap();
        if envsubst::is_templated(&content) {
            content = envsubst::substitute(&content, &std::env::vars().collect()).unwrap();
        }
        match toml::from_str(&content) {
            Ok(c) => Ok(c),
            Err(err) => Err(Box::new(err))
        }
    }
    pub fn create<A: args::Args>(args: &A) -> Result<Config, Box<dyn Error>> {
        let verbosity = args.occurence_of("verbosity");
        simple_logger::init_with_level(match verbosity {
            0 => log::Level::Info,
            1 => log::Level::Debug,
            2 => log::Level::Trace,
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
        c.runtime = RuntimeConfig {
            verbosity,
            dryrun: args.occurence_of("dryrun") > 0,
            debug: match args.values_of("debug") {
                Some(s) => s,
                None => vec!{}
            },
            release_target: {
                // because vcs_service create object which have reference of `c` ,
                // scope of `vcs` should be narrower than this function,
                // to prevent `assignment of borrowed value` error below.
                let vcs = c.vcs_service()?;
                vcs.release_target()
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
    pub fn endpoints_path(&self, release_target: Option<&str>) -> path::PathBuf {
        let p = path::Path::new(&self.common.data_dir).join("endpoints");
        if let Some(e) = release_target {
            log::info!("ep1:{}", e);
            return p.join(format!("{}.toml", e));
        } else if let Some(e) = self.release_target() {
            log::info!("ep2:{}", e);
            return p.join(format!("{}.toml", e));
        } else {
            return p;
        }
    }
    pub fn project_id(&self) -> &str {
        return self.cloud.terraformer.project_id()
    }
    pub fn root_domain(&self) -> &str {
        return self.cloud.terraformer.root_domain()
    }
    pub fn release_target(&self) -> Option<&str> {
        return match &self.runtime.release_target {
            Some(s) => Some(&s),
            None => None
        }
    }
    pub fn canonical_name(&self, prefixed_name: &str) -> String {
        return format!("{}-{}-{}", self.project_id(), 
            self.release_target().expect("should be on release target branch"),
            prefixed_name
        )
    }
    pub fn service_endpoint_version(&'a self, service: &str) -> Result<u32, Box<dyn Error>> {
        match endpoints::Endpoints::load(&self.endpoints_path(None)) {
            Ok(ep) => match ep.releases.get("curr").unwrap().versions.get(&service.to_string()) {
                Some(v) => Ok(*v),
                None => Ok(0), // not deployed yet
            },
            Err(err) => Err(err)
        }
    }
    pub fn update_service_endpoint_version(&self, service: &str) -> Result<u32, Box<dyn Error>> {
        endpoints::Endpoints::modify(&self.endpoints_path(None), |ep| {
            let r = ep.releases.get_mut("next").unwrap();
            let v = r.versions.entry(service.to_string()).or_insert(0);
            *v += 1;
            return Ok(*v);
        })
    }
    pub fn cloud_service(&'a self) -> Result<Box<dyn cloud::Cloud<'a> + 'a>, Box<dyn Error>> {
        return cloud::factory(&self);
    }
    pub fn cloud_region(&'a self) -> &str {
        return self.cloud.terraformer.region();
    }
    pub fn cloud_resource_name(&self, path: &str) -> Result<String, Box<dyn Error>> {
        return Ok("hoge".to_string());
    }
    pub fn vcs_service(&'a self) -> Result<Box<dyn vcs::VCS<'a> + 'a>, Box<dyn Error>> {
        return vcs::factory(&self);
    }
}
