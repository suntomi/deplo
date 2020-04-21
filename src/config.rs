use std::fs;
use serde::{Deserialize, Serialize};
use serde_json::Result;
use clap::{ArgMatches};

#[derive(Serialize, Deserialize)]
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
pub struct ClientBuildConfig {
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

    pub platform_build_configs: Vec<PlatformBuildConfig>
}
#[derive(Serialize, Deserialize)]
pub struct CommonConfig {
    pub deplo_image: String,
    pub script_dir: String,
    pub no_confirm_for_prod_deploy: bool,
}
#[derive(Serialize, Deserialize)]
#[derive(Default)]
pub struct CliConfig {
    pub verbosity: u64,
    pub dryrun: bool,
    pub debug: String,
}
#[derive(Serialize, Deserialize)]
pub struct Config {
    #[serde(skip)]
    pub cli: CliConfig,
    pub common: CommonConfig,
    pub cloud: CloudConfig,
    pub vcs: VCSConfig,
    pub ci: CIConfig,
    pub stores: Vec<StoreConfig>,
    pub client_build: ClientBuildConfig,
}

impl Config {
    // static factory methods 
    pub fn load(path: &str) -> Result<Config> {
        let mut content = fs::read_to_string(path).unwrap();
        if envsubst::is_templated(&content) {
            content = envsubst::substitute(&content, &std::env::vars().collect()).unwrap();
        }
        return serde_json::from_str(&content);
    }
    pub fn create(args: ArgMatches) -> Result<Config> {
        let mut c = Config::load(args.value_of("config").unwrap_or("./deplo.json")).unwrap();
        c.cli = CliConfig {
            verbosity: args.occurrences_of("verbosity"),
            dryrun: args.occurrences_of("dryrun") > 0,
            debug: match args.value_of("debug") {
                Some(s) => s.to_string(),
                None => "".to_string()
            }
        };
        return Ok(c);
    }

    // instance methods
}
