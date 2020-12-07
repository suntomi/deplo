use std::error::Error;
use std::result::Result;
use std::ffi::OsStr;

use maplit::hashmap;
use chrono::Utc;

use crate::config;
use crate::plan;
use crate::builder;
use crate::shell;
use crate::module;
use crate::util::{escalate, write_file};


pub struct Unity<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub builder_config: plan::BuilderConfig,
    pub shell: S,
    pub timestamp: String
}

impl<'a, S: shell::Shell> module::Module for Unity<S> {
    fn prepare(&self, _: bool) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}

impl <'a, S: shell::Shell> Unity<S> {
    fn provision_id(siging_privision_file_path: &str) -> String {
        "".to_string()
    }
    fn batch_method_env<I, K, V>(
        platform_build_config: &plan::UnityPlatformBuildConfig
    ) -> Result<I, Box<dyn Error>>
    where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr> {
        return match platform_build_config {
            plan::UnityPlatformBuildConfig::IOS {
                team_id,
                numeric_team_id:_,
                signing_password:_,
                signing_plist_path:_,
                signing_p12_path:_,
                singing_provision_path,
                automatic_sign,
            } => {
                hashmap!{
                    "DEPLO_UNITY_IOS_TEAM_ID" => team_id,
                    "DEPLO_UNITY_IOS_PROVISION_ID" => Self::provision_id(singing_provision_path),
                    "DEPLO_UNITY_IOS_AUTOMATIC_SIGN" => automatic_sign.unwrap_or(false),
                }
            },
            plan::UnityPlatformBuildConfig::Android {
                keystore_password,
                keyalias_name,
                keyalias_password,
                keystore_path,
                use_expansion_file,
            } => {
                hashmap!{
                    "DEPLO_UNITY_ANDROID_KEYSTORE_PATH" => keystore_path,
                    "DEPLO_UNITY_ANDROID_KEYSTORE_PASSWORD" => keystore_password,
                    "DEPLO_UNITY_ANDROID_KEYALIAS_NAME" => keyalias_name,
                    "DEPLO_UNITY_ANDROID_KEYALIAS_PASSWORD" => keyalias_password,
                    "DEPLO_UNITY_ANDROID_USE_EXPANSION_FILE" => use_expansion_file.to_str()
                }
            },
            _ => return escalate!(Box::new(builder::BuilderError{
                cause: format!("platform type unmatched: {:?}", platform_build_config)
            }))
        }
    }
}

impl<'a, S: shell::Shell> builder::Builder for Unity<S> {
    fn new(
        config: &config::Container, builder_config: &plan::BuilderConfig
    ) -> Result<Unity<S>, Box<dyn Error>> {
        return Ok(Unity::<S> {
            config: config.clone(),
            builder_config: (*builder_config).clone(),
            shell: S::new(config),
        });
    }
    fn build(
        &self,
        build_number: u32,
        version: &str,
        org_name: &str,
        app_name: &str,
        app_id: &str,
        project_path: &str,
        artifact_path: Option<&String>
    ) -> Result<(), Box<dyn Error>> {
        match &self.builder_config {
            plan::BuilderConfig::Unity { 
                unity_version,
                batch_method_name,
                define,
                serial_code,
                account,
                password,
                platform
            } => {
                let execute_method = if batch_method_name.is_none() {
                    write_file(format!("{}/Assets/Deplo/Editor/Builder.cs", project_path), || {
                        include_str!("../../rsc/scripts/build/Builder.cs")
                    });
                    "DeploBuilder.Build"
                } else {
                    batch_method_name.unwrap()
                };
                // player build
                self.shell.eval(include_str!("../../rsc/scripts/build/unity-build.sh"), hashmap!{
                    "unity_version" => unity_version,
                    "project_path" => &project_path.to_string(),
                    "export_path" => artifact_path.unwrap_or(&project_path.to_string()),
                    "execute_method" => execute_method,
                    "build_target" => match platform {
                        plan::UnityPlatformBuildConfig::IOS {..} => { &"iOS".to_string() },
                        plan::UnityPlatformBuildConfig::Android {..} => { &"Android".to_string() },
                        _ => return escalate!(Box::new(builder::BuilderError{
                            cause: format!("platform type unmatched: {:?}", platform)
                        }))
                    },
                    "environment" => &self.config.borrow().release_target().unwrap().to_string(),
                    "define" => define,
                    "timestamp" => &Utc::now().to_rfc3339(),
                    "unity_serial_code" => serial_code,
                    "unity_account" => account,
                    "unity_password" => password
                }.into_iter().chain(hashmap!{
                    "DEPLO_UNITY_COMPANY_NAME" => org_name,
                    "DEPLO_UNITY_APP_NAME" => app_name,
                    "DEPLO_UNITY_APP_ID" => app_id,
                    "DEPLO_UNITY_SCRIPTING_DEFINE_SYMBOLS" => define,
                    "DEPLO_UNITY_APP_VERSION" => version,
                    "DEPLO_UNITY_INTERNAL_BUILD_NUMBER" => build_number,
                }).chain(self.batch_method_env(
                    platform
                )?).collect(), false)?;
                // additional build step for platform specific
                match platform {
                    plan::UnityPlatformBuildConfig::IOS {
                        team_id,
                        numeric_team_id,
                        signing_password,
                        signing_plist_path,
                        signing_p12_path,
                        singing_provision_path, 
                        automatic_sign,        
                    } => {

                    },
                    plan::UnityPlatformBuildConfig::Android {..} => {
                        log::debug!("no platform specific build step")
                    },
                    _ => return escalate!(Box::new(builder::BuilderError{
                        cause: format!("platform type unmatched: {:?}", platform)
                    }))
                }
            },
            _ => return escalate!(Box::new(builder::BuilderError{
                cause: format!("builder type unmatched: {:?}", self.builder_config)
            }))
        }
        Ok(())
    }
}
