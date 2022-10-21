use std::collections::HashMap;
use std::error::Error;

use crate::config;
use crate::module;
use crate::shell;
use crate::step;

pub struct ModuleRunner<S: shell::Shell = shell::Default> {
    config: config::Container,
    module_key: String,
    shell: S
}

impl<S: shell::Shell> step::Step for ModuleRunner<S> {
    fn new(config: &config::Container, module_key: String) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            config: config.clone(),
            module_key,
            shell: S::new(config)
        })
    }
    fn run(
        &self, shell_settings: &shell::Settings,
        envs: &HashMap<String, config::Value>,
        with: &Option<HashMap<String, config::AnyValue>>
    ) -> Result<String, Box<dyn Error>> {
        let c = self.config.borrow();
        let module = c.modules.repos().get(&self.module_key);
        module.run(module::EntryPointType::Step, &self.shell, shell_settings, vec![], shell::mctoa(envs), with)
    }
}
