use std::{thread, time};
use std::error::Error;
use std::result::Result;
use std::collections::HashMap;
use std::fs;

use regex::Regex;
use maplit::hashmap;

use crate::config;
use crate::endpoints;
use crate::shell;
use crate::cloud;
use crate::command::service::plan;

pub struct Gcp<'a, S: shell::Shell<'a> = shell::Default<'a>> {
    pub config: &'a config::Config<'a>,
    pub service_account: String,
    pub gcp_project_id: String,
    pub shell: S,
}

impl<'a, S: shell::Shell<'a>> Gcp<'a, S> {
    // helpers
    fn container_repository_url(&self) -> Option<String> {
        let region = self.config.cloud_region();
        let re = Regex::new(r"([^-]+)-").unwrap();
        return match re.captures(region) {
            Some(c) => Some(
                format!("{}.gcr.io", c.get(1).map_or("", |m| m.as_str()).to_string())
            ),
            None => None
        }
    }
    fn service_account(config: &'a config::Config, shell: &S) -> Result<String, Box<dyn Error>> {
        if let config::CloudProviderConfig::GCP{key} = &config.cloud.provider {
            return Ok(shell.eval_output_of(&format!(r#"
                echo '{}' | jq -jr ".client_email"
            "#, key), &hashmap!{})?)
        }
        return Err(Box::new(config::ConfigError{
            cause: format!("should have GCP config for config.cloud.provider, but {}", config.cloud.provider)
        }))
    }
    fn gcp_project_id(config: &'a config::Config, shell: &S) -> Result<String, Box<dyn Error>> {
        if let config::CloudProviderConfig::GCP{key} = &config.cloud.provider {
            return Ok(shell.eval_output_of(&format!(r#"
                echo '{}' | jq -jr ".project_id"
            "#, key), &hashmap!{})?)
        }
        return Err(Box::new(config::ConfigError{
            cause: format!("should have GCP config for config.cloud.provider, but {}", config.cloud.provider)
        }))
    }
    fn instance_template_name(&self, plan: &plan::Plan, version: u32) -> String {
        return self.config.canonical_name(&format!("instance-template-{}-{}", plan.service, version))
    }
    fn instance_group_name(&self, plan: &plan::Plan, version: u32) -> String {
        return self.config.canonical_name(&format!("instance-group-{}-{}", plan.service, version))
    }
    fn instance_prefix(&self, plan: &plan::Plan, version: u32) -> String {
        return self.config.canonical_name(&format!("instance-{}-{}", plan.service, version))
    }
    fn backend_service_name(&self, plan: &plan::Plan, name: &str, version: u32) -> String {
        if name.is_empty() {
            return self.config.canonical_name(&format!("backend-service-{}-{}", plan.service, version))
        } else {
            return self.config.canonical_name(&format!("backend-service-{}-{}-{}", plan.service, name, version))
        }
    }
    fn serverless_service_name(&self, plan: &plan::Plan) -> String {
        return self.config.canonical_name(&format!("serverless-{}", plan.service))
    }
    fn subscription_name(&self, topic_name: &str) -> String {
        return format!("{}-subscription", topic_name);
    }
    fn health_check_name(&self, plan: &plan::Plan, name: &str, version: u32) -> String {
        return format!("{}-health-check", self.backend_service_name(plan, name, version))
    }
    fn firewall_rule_name(&self, plan: &plan::Plan) -> String {
        return self.config.canonical_name(&format!("fw-rules-{}", plan.service))
    }
    fn url_map_name(&self) -> String {
        return self.config.canonical_name("url-map")
    }
    fn path_matcher_name(&self, endpoints_version: u32) -> String {
        return self.config.canonical_name(&format!("path-matcher-{}", endpoints_version))
    }
    fn metadata_backend_bucket_name(&self, endpoint_version: u32) -> String {
        return self.config.canonical_name(&format!("backend-service-metadata-{}", endpoint_version));
    }
    fn backend_bucket_name(&self, plan: &plan::Plan) -> String {
        return self.config.canonical_name(&format!("backend-bucket-{}", plan.service))
    }
    fn host_rule_add_option_name(&self, url_map_name: &str, target_host: &str) -> Result<&str, Box<dyn Error>> {
        let host_rules = self.shell.eval_output_of(&format!(
            r#"gcloud compute url-maps describe {} --format=json | jq ".hostRules""#, url_map_name
        ), &hashmap!{})?;
        if host_rules.is_empty() {
            return Ok("new-hosts")
        }
        if host_rules == "null" {
            return Ok("new-hosts")
        }
        let host_group = self.shell.eval_output_of(&format!(
            r#"echo {} | jq ".[].hosts[]" | grep "{}""#, host_rules, target_host
        ), &hashmap!{})?;
        if host_group.is_empty() {
            return Ok("new-hosts")
        }
        return Ok("existing-host")
    }
    fn service_path_rule(&self, endpoints: &endpoints::Endpoints) -> Result<String, Box<dyn Error>> {
        let mut rules = vec!();
        let services = endpoints.releases.get("curr").unwrap().versions.keys();
        for s in services {
            let plan = plan::Plan::load(self.config, s)?;
            if plan.has_bluegreen_deployment()? {
                let current_version = endpoints.get_version("curr", s);
                let next_version = endpoints.get_version("next", s);
                let prev_version = endpoints.get_version("prev", s);
                let ports = plan.ports()?.expect("container deployment should have at least an exposed port");
                for (name, _) in &ports {
                    let curr_backend_service = self.backend_service_name(&plan, name, current_version);
                    let next_backend_service = self.backend_service_name(&plan, name, next_version);
                    let prev_backend_service = self.backend_service_name(&plan, name, prev_version);
                    if current_version == next_version {
                        if next_version == 0 {
                            // no service deployed. so no path_rule here. 
                            continue
                        } else if next_version == prev_version {
                            // all same version, only single backend required
                            rules.push(format!("/{}/{}/*={}", s, next_version, next_backend_service));
                        } else {
                            // prev version still points old endpoint. anchor prev
                            rules.push(format!("/{}/{}/*={},/{}/{}/*={}", 
                                s, next_version, next_backend_service,
                                s, prev_version, prev_backend_service));
                        }
                    } else {
                        if current_version == 0 {
                            // next_version is first version to deploy 
                            rules.push(format!("/{}/{}/*={}", s, next_version, next_backend_service));
                        } else {
                            rules.push(format!("/{}/{}/*={},/{}/{}/*={}", 
                                s, current_version, curr_backend_service, 
                                s, next_version, next_backend_service));
                        }
                    }
                }
            }
        }
        Ok(rules.join(","))
    }
    fn bucket_path_rule(
        &self, endpoints: &endpoints::Endpoints, endpoints_version: u32
    ) -> Result<String, Box<dyn Error>> {
        let mut rules = vec!();
        let services = endpoints.releases.get("curr").unwrap().versions.keys();
        rules.push(format!("/meta/*={}", self.metadata_backend_bucket_name(endpoints_version)));
        for s in services {
            let plan = plan::Plan::load(self.config, s)?;
            if !plan.has_bluegreen_deployment()? {
                rules.push(format!("/{}/*={}", s, self.backend_bucket_name(&plan)));
            }
        }
        Ok(rules.join(","))
    }

    fn firewall_option_for_backend(&self, plan: &plan::Plan) -> String {
        let fw_name=self.firewall_rule_name(plan);
        let bs_list=self.shell.eval_output_of(&format!(r#"
            gcloud compute security-policies list --format=json | 
            jq -jr ".[]|select(.name==\"{}\")"
        "#, fw_name), &hashmap!{}).unwrap_or("".to_string());
        if bs_list.is_empty() {
            return "".to_string();
        }
        if bs_list == "null".to_string() {
            return "".to_string();
        }
        return format!("--security-policy={}", fw_name);
    }
    fn backend_added(&self, backend_service_name: &str, instance_group_name: &str) -> bool {
        let bs_list = self.shell.eval_output_of(&format!(r#"
            gcloud compute backend-services list --format=json | 
            jq -jr ".[]|select(.name==\"{}\").backends"
        "#, backend_service_name), &hashmap!{}).unwrap_or("".to_string());
        if bs_list.is_empty() {
            return false;
        }
        if bs_list == "null".to_string() {
            return false;
        }
        let bs_group = self.shell.eval_output_of(&format!(r#"
            echo {} | jq -jr ".[]|select(.group|endswith(\"{}\"))|.group"
        "#, bs_list, instance_group_name), &hashmap!{}).unwrap_or("".to_string());
        if bs_group.is_empty() {
            return false;
        }
        return true;
    }    
    fn instance_list(&self, 
        instance_group_name: &str, resource_location_flag: &str
    ) -> Result<String, Box<dyn Error>> {
        let mut list: String;
        loop {
            list = self.shell.eval(&format!(r#"
                gcloud compute instance-groups list-instances {} {} --format=json |
                jq -jr '[.[].instance]|join(",")'
            "#, instance_group_name, resource_location_flag), &hashmap!{}, true)?;
            if !list.is_empty() {
                return Ok(list)
            }
            thread::sleep(time::Duration::from_secs(5))
        }
    }
    fn wait_serverless_deployment_finish(&self, service_name: &str) -> Result<(), Box<dyn Error>> {
        print!("waiting {} become active.", service_name);
        loop {
            let out = self.shell.eval_output_of(&format!(r#"
                gcloud beta run services list --platform=managed --format=json | 
                jq -jr ".[]|select(.metadata.name == \"{}\")"
            "#, service_name), &hashmap!{})?;
            if out.is_empty() {
                continue
            }
            let status_type=self.shell.eval_output_of(&format!(r#"
                echo '{}' | 
                jq -jr ".status.conditions[0].type"
            "#, out), &hashmap!{})?;
            if status_type == "Ready" && status_type == "True" {
                println!("done.");
                return Ok(())
            } else {
                log::error!("------ cloud run deploy status ------");
                let status=self.shell.eval_output_of(&format!(r#"
                    echo '{}' | jq -jr ".status"
                "#, out), &hashmap!{})?;
                log::error!("=> {}:{}", status, status_type);
                thread::sleep(time::Duration::from_secs(5));
            }
            print!(".")
        }
    }
    fn get_serverless_service_url(&self, service_name: &str) -> Result<String, Box<dyn Error>> {
        let out = self.shell.eval_output_of(&format!(r#"
            gcloud beta run services list --platform=managed --format=json | 
            jq -jr ".[]|select(.metadata.name == \"{}\")
        "#, service_name), &hashmap!{})?;
        if out.is_empty() {
            return Err(Box::new(cloud::CloudError{
                cause: format!(
                    "invalid service_name:{} correspond service does not exist", 
                    service_name)
            }))
        }
        Ok(self.shell.eval_output_of(&format!(r#"
            echo '{}' | jq -jr ".status.address.url"
        "#, out), &hashmap!{})?)
    }
    fn subscribe_topic(
        &self,
        service_name: &str, subscribe_name: &str, topic_name: &str,
        url: Option<&str>
    ) -> Result<(), Box<dyn Error>> {
        let default_url = self.get_serverless_service_url(service_name)?;
        let endpoint_url = url.unwrap_or(&default_url);
        let out = self.shell.eval_output_of(&format!(r#"
            gcloud beta pubsub subscriptions list --format=json | 
            jq -jr ".[]|select(.name|endswith(\"{}\"))
        "#, subscribe_name), &hashmap!{})?;
        if !out.is_empty() {
            let curr_url = self.shell.eval_output_of(&format!(r#"
                echo {} | jq -jr ".pushConfig.pushEndpoint"
            "#, out), &hashmap!{})?;
            let re = Regex::new(&format!("^{}/?$", endpoint_url)).unwrap();
            match re.captures(&curr_url) {
                Some(_) => {
                    log::info!("{}: already subscribe to {}. skipped", subscribe_name, endpoint_url);
                    return Ok(())
                },
                None => {}
            }
        }
        self.shell.eval(&format!("
            gcloud beta pubsub subscriptions create {} --topic {} \
            --push-endpoint={}/ \
            --push-auth-service-account={}
        ", subscribe_name, topic_name, endpoint_url, self.service_account), 
        &hashmap!{}, false)?;

        Ok(())
    }
    // create instance group
    fn deploy_instance_group(
        &self, plan: &plan::Plan,
        image: &str, ports: &HashMap<String, u32>,
        env: &HashMap<String, String>,
        options: &HashMap<String, String>
    ) -> Result<(), Box<dyn Error>> {
        // default value as lazy static
        lazy_static! {
            static ref DEFAULT_SCALING_CONFIG: String = "\
                --max-num-replicas=16 \
                --min-num-replicas=1 \
                --scale-based-on-cpu \
                --target-cpu-utilization=0.8".to_string();
            static ref DEFAULT_CONTINER_OPTIONS: String = "".to_string();
        }
        // settings
        let scaling_config = options.get("scaling_options").unwrap_or(&DEFAULT_SCALING_CONFIG);
        let resource_location_flag = format!("--region={}", self.config.cloud_region());
        let node_distribution = options.get("node_distribution").unwrap_or(&resource_location_flag);
        let container_options = options.get("container_options").unwrap_or(&DEFAULT_CONTINER_OPTIONS);
        let service_version = self.config.next_service_endpoint_version(&plan.service)?;
        let deploy_path = format!("/{}/{}", plan.service, service_version);
        let release_target = self.config.release_target().expect("should be on release target branch");
        let mut env_vec: Vec<String> = Vec::<String>::new();
        env_vec.push(format!("DEPLO_RELEASE_TARGET={}", 
            release_target
        ));
        env_vec.push(format!("DEPLO_SERVICE_VERSION={}", service_version));
        env_vec.push(format!("DEPLO_SERVICE_NAME={}", plan.service));
        env_vec.push(format!("DEPLO_SERVICE_PATH={}", deploy_path));
        for (k, v) in env {
            env_vec.push(format!("{}={}", k, v));
        }

        // generated names
        let instance_template_name = self.instance_template_name(plan, service_version);
        let instance_group_name = self.instance_group_name(plan, service_version);
        let instance_prefix = self.instance_prefix(plan, service_version);
        let service_account = &self.service_account;
        let network = self.config.cloud_resource_name(
            "name@module.vpc.google_compute_network.vpc-network[\"dev\"]"
        )?;
    
        // codes
        log::info!("---- deploy_instance_group");
        let tmpl_id = self.shell.eval_output_of(&format!(r#"
            gcloud compute instance-templates list --format=json |
            jq -jr ".[] |
            select(.name == \"{}\") |
            .id"
        "#, instance_template_name), &hashmap!{})?;
        if tmpl_id.is_empty() {
            log::info!("---- instance template:{} does not exist. create new", instance_template_name);
            self.shell.eval(&format!("gcloud compute instance-templates create-with-container {} \
                --container-image {} --service-account={} \
                --network={} \
                --scopes=cloud-platform \
                --tags http-server,https-server \
                --container-restart-policy on-failure \
                --container-env {} \
                {}
            ", instance_template_name, image, service_account, network, env_vec.join(","), container_options), 
            &hashmap!{}, false)?;
        }
        let ig_id = self.shell.eval_output_of(&format!(r#"
            gcloud compute instance-groups list --format=json | 
            jq -jr ".[] | 
            select(.name == \"{}\") | .id"
        "#, instance_group_name), &hashmap!{})?;
        if ig_id.is_empty() {
            log::info!("---- instance group:{} does not exist. create new", instance_group_name);
            self.shell.eval(&format!("gcloud compute instance-groups managed create {} \
                {} --base-instance-name {} --size {} --template {}
            ", instance_group_name, node_distribution, instance_prefix, 1, instance_template_name), 
            &hashmap!{}, false)?;
        } else {
            log::info!("---- instance group:{} exists. update with new image", instance_group_name);
            self.shell.eval(&format!("gcloud compute instance-groups managed set-instance-template {} \
                --template {} {}
            ", instance_group_name, instance_template_name, resource_location_flag), 
            &hashmap!{}, false)?;
            let instance_list = self.instance_list(&instance_group_name, &resource_location_flag)?;
            self.shell.eval(&format!("gcloud compute instance-groups managed recreate-instances {} \
                --instances {} {}
            ", instance_group_name, instance_list, resource_location_flag), 
            &hashmap!{}, false)?;
        }
        log::info!("---- set named port for {}", instance_group_name);
        self.shell.eval(&format!("gcloud compute instance-groups set-named-ports {} \
            {} \
            --named-ports={}", 
            instance_group_name, resource_location_flag, 
            ports.iter().map(
                |p| format!("{}:{}", if p.0.is_empty() { "primary-port" } else { p.0 }, p.1)
            ).collect::<Vec<String>>().join(",")
        ), 
        &hashmap!{}, false)?;
        log::info!("---- set autoscaling settings for {}", instance_group_name);
        self.shell.eval(&format!("gcloud compute instance-groups managed set-autoscaling {} \
            {} {}
        ", instance_group_name, resource_location_flag, scaling_config), &hashmap!{}, false)?;
        Ok(())
    }
    // deploy container to backend service
    fn deploy_backend_service(
        &self, plan: &plan::Plan,
        name: &str
    ) -> Result<(), Box<dyn Error>> {
        let service_version = self.config.next_service_endpoint_version(&plan.service)?;
        let backend_service_name = self.backend_service_name(plan, name, service_version);
        let health_check_name = self.health_check_name(plan, name, service_version);
        let instance_group_name = self.instance_group_name(plan, service_version);
        let instance_group_type = format!("--instance-group-region={}", self.config.cloud_region());
    
        log::info!("---- deploy_backend_sevice");
        let hc_id = self.shell.eval_output_of(&format!(r#"gcloud compute health-checks list --format=json | 
            jq -jr ".[] | 
            select(.name == \"{}\") | .id"
        "#, health_check_name), &hashmap!{})?;
        if hc_id.is_empty() {
            log::info!("---- health check:{} does not exist. create new", health_check_name);
            self.shell.eval(&format!("gcloud compute health-checks create http {} \
                --port-name={} --request-path=/ping
            ", health_check_name, backend_service_name), &hashmap!{}, false)?;
        }
        let bs_id=self.shell.eval_output_of(&format!(r#"gcloud compute backend-services list --format=json | 
            jq -jr ".[] | 
            select(.name == \"{}\") | .id"
        "#, backend_service_name), &hashmap!{})?;
        if bs_id.is_empty() {
            log::info!("---- backend service:{} does not exist. create new", backend_service_name);
            self.shell.eval(&format!("gcloud compute backend-services create {} \
                --connection-draining-timeout=10 --health-checks={} \
                --protocol=HTTP --port-name={} --global {}
            ", backend_service_name, health_check_name, backend_service_name, ""), 
            &hashmap!{}, false)?;
        }
        let backend_added=self.backend_added(&backend_service_name, &instance_group_name);
        if !backend_added {
            log::info!("---- add backend instance group {} to {}", instance_group_name, backend_service_name);
            self.shell.eval(&format!("gcloud compute backend-services add-backend {} --instance-group={} \
                --global {}
            ", backend_service_name, instance_group_name, instance_group_type), &hashmap!{}, false)?;
        }
        log::info!("---- update backend service balancing setting {}", backend_service_name);
        // TODO: found proper settings for balancing
        self.shell.eval(&format!("gcloud compute backend-services update-backend {} --instance-group={} \
            --balancing-mode=UTILIZATION --global {}
        ", backend_service_name, instance_group_name, instance_group_type), &hashmap!{}, false)?;
        let fw_option=self.firewall_option_for_backend(plan);
        if !fw_option.is_empty() {
            log::info!("---- update backend to use firewall {}", fw_option);
            self.shell.eval(&format!("
                gcloud compute backend-services update {} {} --global
            ", backend_service_name, fw_option), &hashmap!{}, false)?;
        }
        Ok(())
    }
    // deploy container to cloud run
    fn deploy_serverless(
        &self, plan: &plan::Plan,
        image: &str,
        env: &HashMap<String, String>,
        options: &HashMap<String, String>
    ) -> Result<(), Box<dyn Error>> {
        // default value as lazy static
        lazy_static! {
            static ref DEFAULT_MEMORY: String = "1Gi".to_string();
            static ref DEFAULT_EXECUTION_TIMEOUT: String = "15m".to_string();
            static ref DEFAULT_CONTAINER_OPTIONS: String = "".to_string();
        }
        // settings
        let service_name = self.serverless_service_name(plan);
        let region = self.config.cloud_region();
        let service_version = self.config.next_service_endpoint_version(&plan.service)?;
        let mem = options.get("memory").unwrap_or(&DEFAULT_MEMORY);
        let timeout = options.get("execution_timeout").unwrap_or(&DEFAULT_EXECUTION_TIMEOUT);
        let container_options = options.get("container_options").unwrap_or(&DEFAULT_CONTAINER_OPTIONS);
        let subscribed_topic = options.get("subscribed_topic").unwrap_or(&DEFAULT_CONTAINER_OPTIONS);
        let access_control_options = if options.get("access_control").unwrap_or(&DEFAULT_CONTAINER_OPTIONS) != "any" {
            "--no-allow-unauthenticated"
        } else {
            ""
        };
        let mut env_vec: Vec<String> = Vec::<String>::new();
        env_vec.push(format!("DEPLO_RELEASE_TARGET={}", 
            self.config.release_target().expect("should be on release target branch")
        ));
        env_vec.push(format!("DEPLO_SERVICE_VERSION={}", service_version));
        env_vec.push(format!("DEPLO_SERVICE_NAME={}", plan.service));
        for (k, v) in env {
            env_vec.push(format!("{}={}", k, v));
        }
        // direct traffic 
        match self.shell.eval(&format!("\
                gcloud beta run deploy {} \
                --image={} \
                --platform managed --region={} \
                --memory={} --timeout={} {} \
                {}", 
                service_name, image, region,
                mem, timeout, access_control_options, container_options
            ), &hashmap!{}, false) {
            Ok(_) => log::info!("succeed to deploy container {}", image),
            Err(_) => {
                // seems interrupted
                self.wait_serverless_deployment_finish(&service_name)?;
            }
        }
        // set traffic to latest
        self.shell.eval(&format!("gcloud alpha run services update-traffic {} \
            --platform managed --region={} \
            --to-latest", service_name, region), &hashmap!{}, false)?;
        
        if !subscribed_topic.is_empty() {
            let subscription_name = self.subscription_name(subscribed_topic);
            self.subscribe_topic(&service_name, &subscription_name, subscribed_topic, None)?;
        }
        return Ok(())
    }
    fn get_zone_and_project<'b>(&'b self, dns_zone: &'b str) -> (&'b str, &'b str) {
        let parsed: Vec<&str> = dns_zone.split("@").collect();
        return if parsed.len() > 1 {
            (parsed[0], parsed[1])
        } else {
            (parsed[0], &*self.gcp_project_id)
        };
    }
}

impl<'a, S: shell::Shell<'a>> cloud::Cloud<'a> for Gcp<'a, S> {
    fn new(config: &'a config::Config) -> Result<Gcp<'a, S>, Box<dyn Error>> {
        let shell = S::new(config);
        return Ok(Gcp::<'a, S> {
            config: config,
            service_account: Gcp::<'a, S>::service_account(config, &shell)?,
            gcp_project_id: Gcp::<'a, S>::gcp_project_id(config, &shell)?,
            shell
        });
    }
    fn setup_dependency(&self) -> Result<(), Box<dyn Error>> {
        // ensure project setting is valid
        match std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
            Ok(_) => {},
            Err(std::env::VarError::NotPresent) => { 
                if let config::CloudProviderConfig::GCP{
                    key,
                } = &self.config.cloud.provider {
                    fs::write("/tmp/gcp-secret.json", key)?;
                    // setup env for apps which uses gcloud library
                    std::env::set_var("GOOGLE_APPLICATION_CREDENTIALS", "/tmp/gcp-secret.json");
                    std::env::set_var("GOOGLE_PROJECT", &self.gcp_project_id);
                    // setup for gcloud cli 
                    self.shell.eval(&format!(
                        "echo '{}' | gcloud auth activate-service-account --key-file=-", key
                    ), &hashmap!{}, false)?;
                    self.shell.eval(&format!(
                        "gcloud config set project {} && \
                        gcloud config set compute/region {} && \
                        gcloud config set run/region {}",
                        &self.gcp_project_id, &self.config.cloud_region(), &self.config.cloud_region()
                    ), &hashmap!{}, false)?;
                    self.shell.exec(&vec!(
                        "gcloud", "services", "enable", "cloudresourcemanager.googleapis.com"
                    ), &hashmap!{}, false)?;
                } else {
                    return Err(Box::new(cloud::CloudError{
                        cause: format!(
                            "should have GCP config but have: {}", 
                            self.config.cloud.provider
                        )
                    }))                    
                }
            }
            Err(std::env::VarError::NotUnicode(f)) => {
                return Err(Box::new(cloud::CloudError{
                    cause: format!("invalid GOOGLE_APPLICATION_CREDENTIALS value {:?}", f)
                }))
            }
        }
        Ok(())        
    }
    fn cleanup_dependency(&self) -> Result<(), Box<dyn Error>> {
        let config::TerraformerConfig::Terraform {
            backend_bucket,
            bucket_prefix: _,
            dns_zone: _,
            region: _
        } = &self.config.cloud.terraformer;
        return match self.shell.exec(
            &vec!("gsutil", "rb", &format!("gs://{}", backend_bucket)), 
            &hashmap!{}, false) {
            Ok(_) => Ok(()),
            Err(err) => match err {
                shell::ShellError::ExitStatus{ status:_ } => Ok(()),
                _ => Err(Box::new(err))
            }
        }
    }
    fn generate_terraformer_config(&self, name: &str) -> Result<String, Box<dyn Error>> {
        match name {
            "terraform.backend" => {
                let config::TerraformerConfig::Terraform {
                    backend_bucket,
                    bucket_prefix: _,
                    dns_zone: _,
                    region: _
                } = &self.config.cloud.terraformer;
                match self.shell.exec(
                    &vec!("gsutil", "mb", &format!("gs://{}", backend_bucket)), 
                    &hashmap!{}, false) {
                    Ok(_) => {},
                    Err(err) => match err {
                        shell::ShellError::ExitStatus{ status:_ } => {},
                        _ => return Err(Box::new(err))
                    }            
                }
                return Ok(format!("\
                    bucket = \"{}\"\n\
                    prefix = \"{}\"\n\
                    credentials = \"/tmp/gcp-secret.json\"\n\
                ", backend_bucket, self.config.common.project_id));
            },
            "terraform.tfvars" => {
                let config::TerraformerConfig::Terraform {
                    backend_bucket:_,
                    bucket_prefix,
                    dns_zone,
                    region
                } = &self.config.cloud.terraformer;
                let root_domain_dns_name = self.root_domain_dns_name(dns_zone)?;
                let zone_and_project = self.get_zone_and_project(dns_zone);
                return Ok(
                    format!(
                        "\
                            root_domain = \"{}\"\n\
                            dns_zone = \"{}\"\n\
                            dns_zone_project = \"{}\"\n\
                            project_id = \"{}\"\n\
                            region = \"{}\"\n\
                            bucket_prefix = \"{}\"\n\
                            envs = [\"{}\"]\n\
                        ",
                        &root_domain_dns_name[..root_domain_dns_name.len()-1], 
                        zone_and_project.0, zone_and_project.1, 
                        self.config.common.project_id, region, 
                        bucket_prefix.as_ref().unwrap_or(&"".to_string()), 
                        self.config.common.release_targets
                            .keys().map(|s| &**s)
                            .collect::<Vec<&str>>().join(r#"",""#)
                    )
                );
            }
            _ => {
                Err(Box::new(cloud::CloudError{
                    cause: format!("invalid terraformer config name: {}", name)
                }))
            }
        }
    }

    // dns
    fn root_domain_dns_name(&self, zone: &str) -> Result<String, Box<dyn Error>> {
        let zone_and_project = self.get_zone_and_project(zone);
        let r = self.shell.eval_output_of(&format!(r#"
            gcloud dns managed-zones list --project={} --format=json |
            jq -jr ".[]|select(.name==\"{}\").dnsName"
        "#, zone_and_project.1, zone_and_project.0), &hashmap!{})?;
        if r.is_empty() {
            return Err(Box::new(cloud::CloudError{
                cause: format!("no such zone: {} [{}]", zone, r)
            }));
        }
        return Ok(r)
    }

    // container
    fn push_container_image(
        &self, src: &str, target: &str
    ) -> Result<String, Box<dyn Error>> {
        self.shell.exec(&vec!("docker", "tag", src, target), &hashmap!{}, false)?;
        let repository_url = self.container_repository_url().expect(
            &format!("invalid region:{}", self.config.cloud_region())
        );
        // authentication
        match &self.config.cloud.provider {
            config::CloudProviderConfig::GCP{key} => {
                self.shell.eval(&format!(
                    "echo '{}' | docker login -u _json_key --password-stdin https://{}",
                    key, repository_url
                ), &hashmap!{}, false)?;
            },
            _ => return Err(Box::new(cloud::CloudError{
                cause: format!("invalid provider config: {}. gcp config requred", 
                    self.config.cloud.provider)
            }))
        }
        let container_image_tag = format!("{}/{}/{}", repository_url, self.gcp_project_id, target);
        self.shell.exec(&vec!("docker", "tag", src,  &container_image_tag), &hashmap!{}, false)?;
        self.shell.exec(&vec!("docker", "push", &container_image_tag), &hashmap!{}, false)?;
        Ok(container_image_tag)
    }
    fn deploy_container(
        &self, plan: &plan::Plan,
        target: &plan::DeployTarget, 
        // note: ports always contain single entry corresponding to the empty string key
        image: &str, ports: &HashMap<String, u32>,
        env: &HashMap<String, String>,
        options: &HashMap<String, String>
    ) -> Result<(), Box<dyn Error>> {
        match target {
            plan::DeployTarget::Instance => {
                self.deploy_instance_group(plan, image, ports, env, options)?;
                for (name, _) in ports {
                    self.deploy_backend_service(plan, name)?;
                }
            },
            plan::DeployTarget::Kubernetes => {
            },
            plan::DeployTarget::Serverless => {
                self.deploy_serverless(plan, image, env, options)?;
            }
        }
        Ok(())
    }

    // storage
    fn create_bucket(
        &self, bucket_name: &str
    ) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn deploy_storage(
        &self, copymap: &HashMap<String, String>
    ) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    fn update_path_matcher(
        &self, endpoints: &endpoints::Endpoints, maybe_endpoints_version: Option<u32>
    ) -> Result<(), Box<dyn Error>> {
        let target = self.config.release_target().expect("should be on release branch");
        let default_backend_option = match &endpoints.default {
            Some(ep) => {
                let plan = plan::Plan::find_by_endpoint(self.config, ep)?;
                let name = if ep == &plan.service { "" } else { ep };
                log::warn!("TODO: support manually set default backend bucket case");
                format!("--default-service={}", 
                    self.backend_service_name(&plan, name, endpoints.get_version("next", ep))
                )
            },
            None => {
                format!("--default-backend-bucket={}", self.config.default_backend())
            }
        };
        let endpoints_version = maybe_endpoints_version.unwrap_or(endpoints.version);
        log::info!("--- update path matcher ({}/{}/{})", target, default_backend_option, endpoints_version);
        let target_host = &endpoints.prefix;
        let url_map_name = self.url_map_name();
        let path_matcher_name = self.path_matcher_name(endpoints_version);
        let service_path_rule = self.service_path_rule(&endpoints)?;
        let bucket_path_rule = self.bucket_path_rule(&endpoints, endpoints_version)?;
        let host_rule_add_option_name = self.host_rule_add_option_name(&url_map_name, target_host)?;
        self.shell.exec(&vec!(
            "gcloud", "compute", "url-maps", "add-path-matcher", &url_map_name,
            &format!("--path-matcher-name={}", path_matcher_name),
            &default_backend_option,
            &format!("--backend-bucket-path-rules={}", bucket_path_rule),
            &format!("--backend-service-path-rules={}", service_path_rule),
            &format!("--{}={}", host_rule_add_option_name, target_host),
            "--delete-orphaned-path-matcher"
        ), &hashmap!{}, false)?;
    
        log::info!("--- update default service");
        self.shell.exec(&vec!(
            "gcloud", "compute", "url-maps", "set-default-service", &url_map_name,
            &default_backend_option, "--global"
        ), &hashmap!{}, false)?;
    
        log::info!("--- waiting for new urlmap being applied");
        let services = endpoints.releases.get("next").unwrap().versions.keys();
        for s in services {
            let plan = plan::Plan::load(self.config, s)?;
            if !plan.has_bluegreen_deployment()? {
                log::debug!("[{}] does not change path. skipped", s);
                continue
            }
            let next_version = endpoints.get_version("net", s);
            if next_version <= 0 {
                continue
            }
            log::info!("wait for [{}]'s next version url being active.", s);
            let mut count = 0;
            loop {
                let status: u32 = self.shell.eval_output_of(&format!(r#"
                    curl https://{}/{}/{}/ping --output /dev/null -w %{{http_code}} 2>/dev/null
                "#, target_host, s, next_version), &hashmap!{})?.parse().unwrap();
                if status == 200 {
                    log::info!("done");
                    break
                } else {
                    count += 1;
                    if count > 360 {
                        log::error!("[{}]:too long to active. abort", s);
                    }
                }
                print!(".");
                std::thread::sleep(std::time::Duration::from_secs(5));
            }
        }
        Ok(())
    }
}
