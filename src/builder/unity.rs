use std::error::Error;
use std::result::Result;

use crate::config;
use crate::plan;
use crate::builder;
use crate::shell;
use crate::module;

pub struct Unity<S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub builder_config: plan::BuilderConfig,
    pub shell: S,
}

impl<'a, S: shell::Shell> module::Module for Unity<S> {
    fn prepare(&self, _: bool) -> Result<(), Box<dyn Error>> {
        Ok(())
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
        org_name: String,
        app_name: String,
        app_id: String,
        project_path: String,
        artifact_path: Option<String>
    ) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}
