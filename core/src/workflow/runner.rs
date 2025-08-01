use std::collections::HashMap;
use std::error::Error;
use std::f64::consts::E;

use crate::config;
use crate::module;
use crate::shell;
use crate::workflow;

pub struct ModuleRunner<S: shell::Shell = shell::Default> {
    config: config::Container,
    module_key: String,
    shell: S
}

impl<S: shell::Shell> workflow::Workflow for ModuleRunner<S> {
    fn new(config: &config::Container, module_key: String) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            config: config.clone(),
            module_key,
            shell: S::new(config)
        })
    }
    fn listen(
        &self,
        shell_settings: &shell::Settings,
        with: &Option<HashMap<String, config::AnyValue>>
    ) -> Result<(), Box<dyn Error>> {
        let c = self.config.borrow();
        let module = c.modules.repos().get(&self.module_key);
        module.run(
            module::EntryPointType::Workflow,
            &self.shell, shell_settings,
            shell::args!["listen"],
            module::empty_env(), with
        )?;
        Ok(())
    }
    fn matches(
        &self,
        event: &str,
        with: &Option<HashMap<String, config::AnyValue>>
    ) -> Result<Option<String>, Box<dyn Error>> {
        let c = self.config.borrow();
        let module = c.modules.repos().get(&self.module_key);
        match module.run(
            module::EntryPointType::Workflow,
            &self.shell, &shell::capture(),
            shell::args!["match", event],
            module::empty_env(), with
        ) {
            Ok(v) => Ok(if v.len() > 0 { Some(v) } else { None }),
            Err(e) => {
                log::debug!("workflow {} does not match by error {:?}", self.module_key, e);
                Ok(None)
            }
        }
    }
}
