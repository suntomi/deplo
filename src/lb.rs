use std::error::Error;
use std::fs;
use std::collections::HashMap;

use maplit::hashmap;
use chrono::Utc;

use crate::config;
use crate::cloud;
use crate::endpoints;

fn meta_pr<'a>(config: &config::Ref, title: &str, label: &str) -> Result<(), Box<dyn Error>> {
    let vcs = config.vcs_service()?;
    let hash = vcs.commit_hash()?;
    let local_branch = vcs.current_branch()?;
    let remote_branch = format!("{}-meta-pr-{}", local_branch, hash);
    vcs.push(
        &remote_branch, &format!("{}: update meta (by {})", label, hash),
        &vec!(&format!(
            "{}/endpoints/{}.json", 
            config.root_path().to_string_lossy(), 
            config.release_target().expect("should be on release branch")
        )),
        &hashmap! {
            "use-lfs" => "no"
        }
    )?;
    vcs.pr(
        &format!("{} @ {}", title, Utc::now().format("%Y/%m/%d %H:%M:%S").to_string()),
        &remote_branch, &local_branch, &hashmap! {}
    )
}
fn backport_pr<'a>(config: &config::Ref, target_branch: &str) -> Result<(), Box<dyn Error>> {
    let vcs = config.vcs_service()?;
    // TODO: detect source branch (currently fixed to master)
    vcs.pr(
        &format!("backport from release @ {}", Utc::now().format("%Y/%m/%d %H:%M:%S").to_string()),
        &vcs.current_branch()?, target_branch, &hashmap! {}
    )
}
fn try_commit_meta<'a>(config: &config::Ref) -> Result<bool, Box<dyn Error>> {
    let vcs = config.vcs_service()?;
    let hash = vcs.commit_hash()?;
    let remote_branch = vcs.current_branch()?;
    vcs.push(
        &remote_branch, &format!("update meta (by {})", hash),
        &vec!(&format!(
            "{}/endpoints/{}.json", 
            config.root_path().to_string_lossy(), 
            config.release_target().expect("should be on release branch")
        )),
        &hashmap! {
            "use-lfs" => "no"
        }
    )
}
fn deploy_meta(
    config: &config::Ref, 
    release_target: &str, 
    lb_name: &str,
    cloud_account_name: &str,
    endpoints: &endpoints::Endpoints
) -> Result<(), Box<dyn Error>> {
    let cloud = config.cloud_service(&cloud_account_name)?;
    let mv = endpoints.version;
    let bucket_name = config.metadata_bucket_name(lb_name, mv);
    cloud.deploy_storage(
        cloud::StorageKind::Metadata {
            version: mv,
            lb_name: lb_name
        },
        &hashmap! {
            format!("{}/endpoints/{}.json", config.root_path().to_string_lossy(), release_target) => 
            cloud::DeployStorageOption {
                destination: format!("{}/meta/data.json", bucket_name),
                permission: None,
                excludes: None,
                max_age: Some(300),
                region: None // metadata's region is default region for the corresponding account
            }
        }
    )
}

pub fn deploy(
    config: &config::Container,
    enable_backport: bool
) -> Result<(), Box<dyn Error>> {
    let config_ref = config.borrow();
    let target = config_ref.release_target().expect("should be on release branch");
    let mut endpoints = endpoints::Endpoints::load(
        config,
        &config_ref.endpoints_file_path(Some(target))
    )?;
    if endpoints.next.is_none() {
        log::warn!("no new release. next should be non-null in {}", endpoints.dump()?);
        return Ok(())
    }
    let mut lb_change_types: HashMap<&str, endpoints::ChangeType> = hashmap!{};
    let mut change_type = endpoints::ChangeType::None;
    for (lb_name, _) in &config_ref.lb {
        let ct = endpoints.change_type(lb_name, &config)?;
        lb_change_types.insert(lb_name, ct.clone());
        if change_type == endpoints::ChangeType::None {
            change_type = ct;
        } else if ct == endpoints::ChangeType::Path {
            change_type = ct;
        }
    }
    let current_metaver = endpoints.version;
    let endpoints_deploy_state = match &endpoints.deploy_state {
        Some(ds) => ds.clone(),
        None => endpoints::DeployState::Invalid
    };
    let confirm_deploy = endpoints.confirm_deploy.unwrap_or(false);
    log::info!("---- deploy_load_balancer change_type({}), endpoints_deploy_state({})",
        change_type, endpoints_deploy_state);
    if change_type != endpoints::ChangeType::None {
        let deployed = !confirm_deploy || 
            endpoints::DeployState::ConfirmCleanup == endpoints_deploy_state;
        if deployed {
            log::info!("--- cascade versions confirm_deploy:{} endpoints_deploy_state:{}", 
                confirm_deploy, endpoints_deploy_state);
            // push next release as latest release
            endpoints.cascade_releases(config)?;
            // and collect releases which is not referenced by distributions
            if endpoints.gc_releases(&config_ref)? {
                // if true, means some releases are collected and path will be changed
                log::info!("--- set change_type to Path");
                change_type = endpoints::ChangeType::Path;
            }
            endpoints.set_deploy_state(&config_ref, None)?;
        } else {
            log::info!("--- set pending cleanup for {} deployment", target);
            endpoints.set_deploy_state(&config_ref, Some(endpoints::DeployState::ConfirmCleanup))?;
            // before PR, next release as also seen in load balancer, because some of workflow
            // requires QA in real environment (eg. game)
        }
        if change_type == endpoints::ChangeType::Path {
            // if any of lb's path changes, meta version will be up
            log::info!("--- {}: meta version up {} => {} due to urlmap changes", 
                target, current_metaver, current_metaver + 1);
            endpoints.version_up(&config_ref)?;
        }
        for (lb_name, ct) in &lb_change_types {
            let lb_config = config_ref.lb_config(lb_name);
            log::info!("--- deploy metadata bucket for {}, change_type={}", lb_name, change_type);
            deploy_meta(&config_ref, target, lb_name, &lb_config.account_name(), &endpoints)?;
            if *ct == endpoints::ChangeType::Path || change_type == endpoints::ChangeType::Path {
                &config_ref.cloud_service(&lb_config.account_name())?.update_path_matcher(lb_name, &endpoints)?;
            }
        }

        if deployed {
            // if any change, commit to github
            try_commit_meta(&config_ref)?;
        } else {
            meta_pr(&config_ref, &format!("update {}.json: cutover new release", target), "cutover")?;
        }
    } else if enable_backport {
        if let Some(b) = &endpoints.backport_target_branch {
            backport_pr(&config_ref, b)?;
        }
    }

    Ok(())
}

pub fn prepare(
    config: &config::Container
) -> Result<(), Box<dyn Error>> {
    let config_ref = config.borrow();
    let root_domain = config_ref.root_domain()?;
    for (k, _) in &config_ref.common.release_targets {
        match fs::metadata(config_ref.endpoints_file_path(Some(k))) {
            Ok(_) => log::debug!("versions file for [{}] already created", k),
            Err(_) => {
                log::info!("create versions file for [{}]", k);
                fs::create_dir_all(&config_ref.endpoints_path())?;
                let ep = endpoints::Endpoints::new(k, &root_domain);
                ep.save(config_ref.endpoints_file_path(Some(k)))?;
            }
        }
    }
    Ok(())
}

pub fn cleanup(
    _: &config::Container
) -> Result<(), Box<dyn Error>> {
    Ok(())
}