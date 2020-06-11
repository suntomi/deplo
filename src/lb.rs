use std::error::Error;

use maplit::hashmap;
use chrono::Utc;

use crate::config;
use crate::cloud;
use crate::endpoints;

fn meta_pr(config: &config::Config, title: &str, label: &str) -> Result<(), Box<dyn Error>> {
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
fn backport_pr(config: &config::Config, target_branch: &str) -> Result<(), Box<dyn Error>> {
    let vcs = config.vcs_service()?;
    // TODO: detect source branch (currently fixed to master)
    vcs.pr(
        &format!("backport from release @ {}", Utc::now().format("%Y/%m/%d %H:%M:%S").to_string()),
        &vcs.current_branch()?, target_branch, &hashmap! {}
    )
}
fn try_commit_meta(config: &config::Config) -> Result<bool, Box<dyn Error>> {
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
fn metadata_bucket_name(
    config: &config::Config, metaver: u32
) -> String {
    config.canonical_name(&format!("metadata-{}",  metaver))
}
fn deploy_meta(
    config: &config::Config, release_target: &str, endpoints: &endpoints::Endpoints
) -> Result<(), Box<dyn Error>> {
    let cloud = config.cloud_service()?;
    let mv = endpoints.version;
    let bucket_name = metadata_bucket_name(config, mv);
    cloud.deploy_storage(
        cloud::StorageKind::Metadata {
            version: mv
        },
        &hashmap! {
            format!("{}/endpoints/{}.json", config.root_path().to_string_lossy(), release_target) => 
            cloud::DeployStorageOption {
                destination: format!("{}/meta/data.json", bucket_name),
                permission: None,
                excludes: None,
                max_age: Some(300)
            }
        }
    )
}

pub fn deploy(
    config: &config::Config,
    enable_backport: bool
) -> Result<(), Box<dyn Error>> {
    let target = config.release_target().expect("should be on release branch");
    let mut endpoints = endpoints::Endpoints::load(
        config,
        &config.endpoints_file_path(Some(target))
    )?;
    let current_metaver = endpoints.version;
    let mut change_type = endpoints.change_type(config)?;
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
        // push next release as latest release
        endpoints.cascade_releases(config)?;
        if deployed {
            log::info!("--- cascade versions confirm_deploy:{} endpoints_deploy_state:{}", 
                confirm_deploy, endpoints_deploy_state);
            if endpoints.gc_releases(config)? {
                // if true, means some releases are collected and path will be changed
                change_type = endpoints::ChangeType::Path;
            }
            endpoints.set_deploy_state(config, None)?;
        } else {
            log::info!("--- set pending cleanup for prod deployment");
            endpoints.set_deploy_state(config, Some(endpoints::DeployState::ConfirmCleanup))?;
            // before PR, pushed next release as also seen in load balancer, because some of workflow
            // requires QA in real environment (eg. game)
        }
        if change_type == endpoints::ChangeType::Path {
            log::info!("--- {}: meta version up {} => {} due to urlmap changes", 
                target, current_metaver, current_metaver + 1);
            endpoints.version_up(config)?;
        }
        log::info!("--- deploy metadata bucket");
        deploy_meta(config, target, &endpoints)?;

        if change_type == endpoints::ChangeType::Path {
            config.cloud_service()?.update_path_matcher(&endpoints)?;
        }

        if deployed {
            // if any change, commit to github
            try_commit_meta(config)?;
        } else {
            meta_pr(config,
                &format!("update {}.json: cutover new release", target),
                "cutover"
            )?;
        }
    } else if enable_backport {
        if let Some(b) = &endpoints.backport_target_branch {
            backport_pr(config, b)?;
        }
    }

    Ok(())
}
