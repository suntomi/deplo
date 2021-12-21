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
    fn make_diff(&self) -> Result<String, Box<dyn Error>> {
        let config = self.config.borrow();
        let (account_name, _) = config.ci_config_by_env();
        let ci_service = config.ci_service(account_name)?;
        let (ref_name, is_branch) = self.git.current_branch()?;
        let mut old_base: Option<String> = None;
        if is_branch && config.ci.rebase_before_diff.unwrap_or(false) {
            // get current base commit hash (that is, HEAD^1). 
            // because below we do rebase, which may change HEAD.
            // but we want to know the diff between "current" HEAD^1 and "rebased" HEAD with ... 
            // to invoke all possible deployment that need to run
            let commit = self.git.commit_hash(None)?;
            old_base = Some(self.git.commit_hash(Some(&format!("{}^", commit)))?);
            self.git.rebase_with_remote_counterpart(&self.push_url()?, &ref_name)?;
        }

        let diff = if !is_branch {
            let tags = self.git.tags()?;
            let index = tags.iter().position(|tag| tag.as_str() == ref_name.as_str()).ok_or(Box::new(vcs::VCSError {
                cause: format!("tag {} does not found for list {:?}", ref_name, tags)
            }))?;
            if index == 0 {
                "*".to_string()
            } else {
                self.git.diff_paths(&format!("{}..{}", &tags[index - 1], &tags[index]))?
            }
        } else {
            match ci_service.pull_request_url()? {
                Some(url) => {
                    if let config::VCSConfig::Github{ email:_, account, key } = &config.vcs {
                        let base = self.git.pr_data(&url, account, key, ".base.ref")?;
                        if base.is_empty() {
                            panic!("fail to get base branch from ${url}", url = url);
                        }
                        self.git.diff_paths(&format!("origin/{}...HEAD", base))?
                    } else {
                        panic!("vcs account is not for github ${:?}", &config.vcs);
                    }
                },
                None => match old_base {
                    Some(v) => self.git.diff_paths(&format!("{}..HEAD", v))?,
                    None => self.git.diff_paths("HEAD^")?
                }
            }
        };
        Ok(diff)
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
        let response = self.shell.eval_output_of(
            &format!(r#"
                curl --fail https://api.github.com/repos/{}/{}/releases/tags/{} \
                -H "Authorization: token {}"
            "#, user_and_repo.0, user_and_repo.1, target_ref.0, token),
            shell::default(), shell::no_env(), shell::no_cwd()
        )?;
        return Ok(response);
    }
    fn get_value_from_json_object(&self, json_object: &str, key: &str) -> Result<String, Box<dyn Error>> {
        let object: JsonValue = serde_json::from_str(json_object)?;
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
            let mut gh = Github {
                config: config.clone(),
                diff: vec!(),
                shell: S::new(config),
                git: GIT::new(&account, &email, config)
            };
            gh.diff = gh.make_diff()?.split('\n').map(|e| e.to_string()).collect();
            return Ok(gh)
        } 
        return Err(Box::new(vcs::VCSError {
            cause: format!("should have github config but {}", config.borrow().vcs)
        }))
    }
    fn release_target(&self) -> Option<String> {        
        let config = self.config.borrow();
        let (b, _) = self.git.current_branch().unwrap();
        for (k,v) in &config.common.release_targets {
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
                self.shell.eval_output_of(
                    &format!(r#"
                        curl https://api.github.com/repos/{}/{}/releases \
                        -H "Authorization: token {}" \
                        -d '{}'
                    "#, user_and_repo.0, user_and_repo.1, token, serde_json::to_string(&options)?),
                    shell::default(), shell::no_env(), shell::no_cwd()
                )?             
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
        let response = self.shell.eval_output_of(
            &format!(r#"
                curl {} \
                -H "Authorization: token {}"
            "#, upload_url, token),
            shell::default(), shell::no_env(), shell::no_cwd()
        )?;
        match jsonpath!(&response, &format!("$$.[@.name==${}]", asset_name)) {
            Ok(v) => match opts.get("replace") {
                Some(_) => {
                    // delete old asset
                    let delete_url = self.get_value_from_json_object(&v, "url")?;
                    self.shell.eval_output_of(
                        &format!(r#"
                            curl {} -X DELETE \
                            -H "Authorization: token {}"
                        "#, delete_url, token),
                        shell::default(), shell::no_env(), shell::no_cwd()
                    )?;
                },
                // nothing to do
                None => return self.get_value_from_json_object(&v, "browser_download_url")
            },
            // seems no asset with this name, proceed to upload
            Err(_) => ()
        };
        let response = self.shell.eval_output_of(
            &format!(r#"
                curl {} \
                -H "Authorization: token {}" \
                -H "Content-Type: {}" \
                --data-binary "@{}"
            "#, upload_url, token, content_type, asset_file_path),
            shell::default(), shell::no_env(), shell::no_cwd()
        )?;  
        self.get_value_from_json_object(&response, "browser_download_url")
    }
    fn rebase_with_remote_counterpart(&self, branch: &str) -> Result<(), Box<dyn Error>> {
        self.git.rebase_with_remote_counterpart(&self.push_url()?, branch)
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
    fn diff<'b>(&'b self) -> &'b Vec<String> {
        &self.diff
    }
}
