use std::error::Error;
use std::fs;

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
    cloud_account_name: &str,
    endpoints: &endpoints::Endpoints
) -> Result<(), Box<dyn Error>> {
    let cloud = config.cloud_service(&cloud_account_name)?;
    let mv = endpoints.version;
    let bucket_name = config.canonical_name(&format!("metadata-{}",  mv));
    cloud.deploy_storage(
        cloud::StorageKind::Metadata {
            version: mv,
            lb_name: &endpoints.lb_name
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

fn deploy_single_load_balancer(
    target: &str,
    config: &config::Ref,
    endpoints: &mut endpoints::Endpoints,
    original_change_type: endpoints::ChangeType,
    enable_backport: bool
) -> Result<(), Box<dyn Error>> {
    let cloud_account_name = endpoints.cloud_account_name(config);
    let current_metaver = endpoints.version;
    let mut change_type = original_change_type;
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
            if endpoints.gc_releases(config)? {
                // if true, means some releases are collected and path will be changed
                change_type = endpoints::ChangeType::Path;
            }
            endpoints.set_deploy_state(config, None)?;
        } else {
            log::info!("--- set pending cleanup for prod deployment");
            endpoints.set_deploy_state(config, Some(endpoints::DeployState::ConfirmCleanup))?;
            // before PR, next release as also seen in load balancer, because some of workflow
            // requires QA in real environment (eg. game)
        }
        if change_type == endpoints::ChangeType::Path {
            log::info!("--- {}: meta version up {} => {} due to urlmap changes", 
                target, current_metaver, current_metaver + 1);
            endpoints.version_up(&config)?;
        }
        log::info!("--- deploy metadata bucket");
        deploy_meta(config, target, &cloud_account_name, &endpoints)?;

        if change_type == endpoints::ChangeType::Path {
            &config.cloud_service(&cloud_account_name)?.update_path_matcher(&endpoints)?;
        }

        if deployed {
            // if any change, commit to github
            try_commit_meta(config)?;
        } else {
            meta_pr(config, &format!("update {}.json: cutover new release", target), "cutover")?;
        }
    } else if enable_backport {
        if let Some(b) = &endpoints.backport_target_branch {
            backport_pr(config, b)?;
        }
    }

    Ok(())
}

pub fn deploy(
    config: &config::Container,
    enable_backport: bool
) -> Result<(), Box<dyn Error>> {
    let config_ref = config.borrow();
    let target = config_ref.release_target().expect("should be on release branch");
    for (lb_name, _) in &config_ref.lb {
        let mut endpoints = endpoints::Endpoints::load(
            config,
            &config_ref.endpoints_file_path(lb_name, Some(target))
        )?;
        let ct = endpoints.change_type(&config)?;
        deploy_single_load_balancer(target, &config_ref, &mut endpoints, ct, enable_backport)?
    }

    Ok(())
}

pub fn prepare(
    config: &config::Container
) -> Result<(), Box<dyn Error>> {
    let config_ref = config.borrow();
    let root_domain = config_ref.root_domain()?;
    for (lb_name, _) in &config_ref.lb {
        for (k, _) in &config_ref.common.release_targets {
            match fs::metadata(config_ref.endpoints_file_path(lb_name, Some(k))) {
                Ok(_) => if lb_name == "default" {
                    log::debug!("versions file for [{}] already created", k)
                } else {
                    log::debug!("versions file for [{}.{}] already created", lb_name, k)
                },
                Err(_) => {
                    let domain = if lb_name == "default" {
                        log::info!("create versions file for [{}]", k);
                        fs::create_dir_all(&config_ref.endpoints_path())?;
                        format!("{}.{}", k, root_domain)
                    } else {
                        log::info!("create versions file for [{}.{}]", lb_name, k);
                        fs::create_dir_all(&config_ref.endpoints_path().join(lb_name))?;
                        format!("{}.{}.{}", k, lb_name, root_domain)
                    };
                    let ep = endpoints::Endpoints::new(lb_name, &domain);
                    ep.save(config_ref.endpoints_file_path(lb_name, Some(k)))?;
                }
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