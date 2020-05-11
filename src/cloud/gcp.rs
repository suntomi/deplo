use std::error::Error;
use std::result::Result;
use std::collections::HashMap;

use crate::config;
use crate::cloud;

pub struct Gcp<'a> {
    pub config: &'a config::Config<'a>
}

impl<'a> cloud::Cloud<'a> for Gcp<'a> {
    fn new(config: &'a config::Config) -> Result<Gcp<'a>, Box<dyn Error>> {
        return Ok(Gcp::<'a> {
            config: config
        });
    }
    fn push_container_image(&self, src: &str, target: &str) -> Result<String, Box<dyn Error>> {
        Ok("".to_string())
    }
    fn deploy_to_autoscaling_group(
        &self, image: &str, ports: &Vec<u32>,
        env: &HashMap<String, String>,
        options: &HashMap<String, String>
    ) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn deploy_to_serverless_platform(
        &self, image: &str, ports: &Vec<u32>,
        env: &HashMap<String, String>,
        options: &HashMap<String, String>
    ) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    fn create_bucket(
        &self, bucket_name: &str
    ) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn deploy_to_storage(
        &self, copymap: &HashMap<String, String>
    ) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

}
