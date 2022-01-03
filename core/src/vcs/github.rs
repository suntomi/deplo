use std::error::Error;
use std::collections::HashMap;
use std::path::Path;
use std::result::Result;

use glob::Pattern;
use regex;
use serde_json::{Value as JsonValue};

use crate::config;
use crate::vcs;
use crate::module;
use crate::shell;
use crate::util::{escalate,make_escalation,jsonpath,str_to_json};

use super::git;

pub struct Github<GIT: (git::GitFeatures) + (git::GitHubFeatures) = git::Git, S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub git: GIT,
    pub shell: S,
    pub diff: Vec<String>
}

impl<GIT: (git::GitFeatures) + (git::GitHubFeatures), S: shell::Shell> Github<GIT, S> {
    fn push_url(&self) -> Result<String, Box<dyn Error>> {
        let config = self.config.borrow();
        if let config::VCSConfig::Github{ email:_, account, key } = &config.vcs {
            let user_and_repo = (self as &dyn vcs::VCS).user_and_repo()?;
            Ok(format!("https://{}:{}@github.com/{}/{}", account, key, user_and_repo.0, user_and_repo.1))
        } else {
            Err(Box::new(vcs::VCSError {
                cause: format!("should have github config, got: {}", config.vcs)
            }))
        }
    }
    fn get_release(&self, target_ref: (&str, bool)) -> Result<String, Box<dyn Error>> {
        if target_ref.1 {
            return escalate!(Box::new(vcs::VCSError {
                cause: format!("release only can create with tag but branch given: {}", target_ref.0)
            }));
        }
        let config = self.config.borrow();
        let token = match &config.vcs {
            config::VCSConfig::Github{ account:_, key, email:_ } => key,
            _ => panic!("vcs account is not for github ${:?}", &config.vcs)
        };
        let user_and_repo = (self as &dyn vcs::VCS).user_and_repo()?;
        let response = self.shell.exec(&vec![
            "curl", "--fail", "-sS", &format!(
                "https://api.github.com/repos/{}/{}/releases/tags/{}",
                user_and_repo.0, user_and_repo.1, target_ref.0
            ), "-H", &format!("Authorization: token {}", token)
        ], shell::no_env(), shell::no_cwd(), true)?;
        return Ok(response);
    }
    fn get_value_from_json_object(&self, json_object: &str, key: &str) -> Result<String, Box<dyn Error>> {
        let mut object: JsonValue = str_to_json(json_object);
        // TODO_JSON: jsonpath does not work intuitively, consider replace it with jq_rs
        if object.is_object() {
        } else if object.is_array() && object.as_array().unwrap().len() > 0 {
            object = object.as_array().unwrap().get(0).unwrap().clone();
        } else {
            return escalate!(Box::new(vcs::VCSError {
                cause: format!("json object is not object or array: {}", json_object)
            }));
        }
        // log::debug!("inspect object: {}", serde_json::to_string(&object)?);
        let value = object[key].as_str().ok_or(make_escalation!(Box::new(vcs::VCSError {
            cause: format!("key [{}] not found in object: {}", key, object)
        })))?;
        Ok(value.to_string())
    }
    fn get_upload_url_from_release(&self, release: &str) -> Result<String, Box<dyn Error>> {
        let upload_url = self.get_value_from_json_object(release, "upload_url")?;
        Ok(regex::Regex::new(r#"\{.*\}$"#).unwrap().replace(&upload_url, "").to_string())
   }
}

impl<GIT: (git::GitFeatures) + (git::GitHubFeatures), S: shell::Shell> module::Module for Github<GIT, S> {
    fn prepare(&self, _:bool) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}

impl<GIT: (git::GitFeatures) + (git::GitHubFeatures), S: shell::Shell> vcs::VCS for Github<GIT, S> {
    fn new(config: &config::Container) -> Result<Github<GIT,S>, Box<dyn Error>> {
        if let config::VCSConfig::Github{ account, key:_, email } = &config.borrow().vcs {
            return Ok(Github {
                config: config.clone(),
                diff: vec!(),
                shell: S::new(config),
                git: GIT::new(&account, &email, config)
            });
        } 
        return Err(Box::new(vcs::VCSError {
            cause: format!("should have github config but {}", config.borrow().vcs)
        }))
    }
    fn release_target(&self) -> Option<String> {        
        let config = self.config.borrow();
        let (b, is_branch) = self.git.current_branch().unwrap();
        for (k,v) in &config.common.release_targets {
            if is_branch && !v.is_branch() {
                continue;
            }
            if !is_branch && !v.is_tag() {
                continue;
            }
            let re = Pattern::new(v.path()).unwrap();
            match re.matches(&b) {
                true => return Some(k.to_string()),
                false => {}, 
            }
        }
        None
    }
    fn release(
        &self, target_ref: (&str, bool), opts: &JsonValue
    ) -> Result<String, Box<dyn Error>> {
        if target_ref.1 {
            return escalate!(Box::new(vcs::VCSError {
                cause: format!("release only can create with tag but branch given: {}", target_ref.0)
            }));
        }
        let response = match self.get_release(target_ref) {
            Ok(v) => v,
            Err(_) => {
                let config = self.config.borrow();
                let token = match &config.vcs {
                    config::VCSConfig::Github{ account:_, key, email:_ } => key,
                    _ => panic!("vcs account is not for github ${:?}", &config.vcs)
                };
                let user_and_repo = self.user_and_repo()?;
                // create release
                let mut options = match opts.as_object() {
                    Some(v) => v.clone(),
                    None => return escalate!(Box::new(vcs::VCSError {
                        cause: format!("options for vcs.release should be JSON object: {:?}", opts)
                    }))
                };
                options.insert("tag_name".to_string(), str_to_json(target_ref.0));
                self.shell.exec(&vec![
                    "curl", "-sS", &format!(
                        "https://api.github.com/repos/{}/{}/releases", 
                        user_and_repo.0, user_and_repo.1
                    ), 
                    "-H", &format!("Authorization: token {}", token), 
                    "-d", &serde_json::to_string(&options)?
                ], shell::no_env(), shell::no_cwd(), false)?             
            }
        };
        let upload_url = self.get_upload_url_from_release(&response)?;
        log::debug!("upload_url: {}", upload_url);
        Ok(upload_url)
    }
    fn release_assets(
        &self, target_ref: (&str, bool), asset_file_path: &str, opts: &JsonValue
    ) -> Result<String, Box<dyn Error>> {
        // target_ref checked in get_release
        let upload_url_base = self.get_upload_url_from_release(&self.get_release(target_ref)?)?;
        let asset_name = match opts.get("name") {
            Some(v) => {
                v.as_str().ok_or(Box::new(vcs::VCSError {
                    cause: format!("options for vcs.release_assets should be JSON object: {:?}", opts)
                }))?.to_string()
            },
            None => Path::new(asset_file_path).file_name().unwrap().to_str().unwrap().to_string()
        };
        let upload_url = format!("{}?name={}", upload_url_base, asset_name);
        let config = self.config.borrow();
        let token = match &config.vcs {
            config::VCSConfig::Github{ account:_, key, email:_ } => key,
            _ => panic!("vcs account is not for github ${:?}", &config.vcs)
        };
        let content_type = match opts.get("content-type") {
            Some(v) => v.as_str().unwrap_or("application/octet-stream").to_string(),
            None => "application/octet-stream".to_string()
        };
        let response = self.shell.exec(&vec![
            "curl", "-sS", &upload_url_base.replace("uploads.github.com", "api.github.com"),
            "-H", &format!("Authorization: token {}", token),
        ], shell::no_env(), shell::no_cwd(), true)?;
        match jsonpath(&response, &format!("$.[?(@.name=='{}')]", asset_name))? {
            Some(v) => match opts.get("replace") {
                Some(_) => {
                    // delete old asset
                    let delete_url = self.get_value_from_json_object(&v, "url")?;
                    self.shell.exec(&vec![
                        "curl", &delete_url, "-X", "DELETE", "-H", &format!("Authorization: token {}", token)
                    ], shell::no_env(), shell::no_cwd(), false)?;
                },
                // nothing to do, return browser_download_url
                None => return self.get_value_from_json_object(&v, "browser_download_url")
            },
            None => log::debug!("no asset with name {}, proceed to upload", asset_name),
        };
        let response = self.shell.exec(&vec![
            "curl", "-sS", &upload_url, "-H", &format!("Authorization: token {}", token),
            "-H", &format!("Content-Type: {}", content_type),
            "--data-binary", &format!("@{}", asset_file_path),
        ], shell::no_env(), shell::no_cwd(), true)?;  
        self.get_value_from_json_object(&response, "browser_download_url")
    }
    fn rebase_with_remote_counterpart(&self, branch: &str) -> Result<(), Box<dyn Error>> {
        self.git.rebase_with_remote_counterpart(&self.push_url()?, branch)
    }
    fn current_ref(&self) -> Result<(vcs::RefType, String), Box<dyn Error>> {
        self.git.current_ref()
    }
    fn current_branch(&self) -> Result<(String, bool), Box<dyn Error>> {
        self.git.current_branch()
    }
    fn commit_hash(&self, expr: Option<&str>) -> Result<String, Box<dyn Error>> {
        self.git.commit_hash(expr)
    }
    fn repository_root(&self) -> Result<String, Box<dyn Error>> {
        self.git.repository_root()
    }
    fn push(
        &self, branch: &str, msg: &str, patterns: &Vec<&str>, options: &HashMap<&str, &str>
    ) -> Result<bool, Box<dyn Error>> {
        self.git.push(&self.push_url()?, branch, msg, patterns, options)
    }
    fn pr(
        &self, title: &str, head_branch: &str, base_branch: &str, options: &HashMap<&str, &str>
    ) -> Result<(), Box<dyn Error>> {
        self.git.pr(title, head_branch, base_branch, options)
    }
    fn user_and_repo(&self) -> Result<(String, String), Box<dyn Error>> {
        let remote_origin = self.git.remote_origin()?;
        let re = regex::Regex::new(r"[^:]+[:/]([^/\.]+)/([^/\.]+)").unwrap();
        let user_and_repo = match re.captures(&remote_origin) {
            Some(c) => (
                c.get(1).map_or("".to_string(), |m| m.as_str().to_string()), 
                c.get(2).map_or("".to_string(), |m| m.as_str().to_string())
            ),
            None => return escalate!(Box::new(vcs::VCSError {
                cause: format!("invalid remote origin url: {}", remote_origin)
            }))
        };
        Ok(user_and_repo)
    }
    fn make_diff(&self) -> Result<String, Box<dyn Error>> {
        let diff = match self.git.current_ref()? {
            (vcs::RefType::Branch, _) => {
                self.git.diff_paths("HEAD^")?
            },
            (vcs::RefType::Pull, _) => {
                self.git.diff_paths("HEAD^")?
            },
            (vcs::RefType::Tag, ref_name) => {
                let tags = self.git.tags()?;
                let index = tags.iter().position(|tag| tag.as_str() == ref_name.as_str()).ok_or(
                    make_escalation!(Box::new(vcs::VCSError {
                        cause: format!("tag {} does not found for list {:?}", ref_name, tags)
                    }))
                )?;
                if index == 0 {
                    // this is first tag, so treat as it changes everyhing
                    "*".to_string()
                } else {
                    // diffing with previous tag
                    self.git.diff_paths(&format!("{}..{}", &tags[index - 1], &tags[index]))?
                }
            },
            (vcs::RefType::Commit, ref_name) => {
                return escalate!(Box::new(vcs::VCSError {
                    cause: format!("current head does not branch or tag {}", ref_name)
                }))
            }
        };
        Ok(diff)
    }    
    fn init_diff(&mut self, diff: String) -> Result<(), Box<dyn Error>> {
        self.diff = diff.split('\n').map(|e| e.to_string()).collect();
        Ok(())
    }
    fn diff<'b>(&'b self) -> &'b Vec<String> {
        &self.diff
    }
}
