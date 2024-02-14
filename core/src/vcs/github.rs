use std::error::Error;
use std::collections::HashMap;
use std::path::Path;
use std::result::Result;

use glob::Pattern;
use maplit::hashmap;
use regex;
use serde_json::{Value as JsonValue};

use crate::config;
use crate::vcs;
use crate::shell;
use crate::util::{escalate,make_escalation,jsonpath,str_to_json};

use super::git;

pub struct Github<GIT: git::GitFeatures<S> = git::Git, S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub git: GIT,
    pub shell: S,
    pub diff: Vec<String>
}

impl<GIT: git::GitFeatures<S>, S: shell::Shell> Github<GIT, S> {
    fn pushable_remote_url(&self) -> Result<String, Box<dyn Error>> {
        let user_and_repo = (self as &dyn vcs::VCS).user_and_repo()?;
        Ok(format!("https://github.com/{}/{}", user_and_repo.0, user_and_repo.1))
    }
    fn with_remote_for_push<F,R>(
        &self, executer: F
    ) -> Result<R, Box<dyn Error>> where F: Fn (&str) -> Result<R, Box<dyn Error>>{
        let url = self.pushable_remote_url()?;
        executer(&url)
    }
    fn get_release(&self, target_ref: (&str, bool)) -> Result<String, Box<dyn Error>> {
        if target_ref.1 {
            return escalate!(Box::new(vcs::VCSError {
                cause: format!("release only can create with tag but branch given: {}", target_ref.0)
            }));
        }
        let config = self.config.borrow();
        let token = match &config.vcs {
            config::vcs::Account::Github{ account:_, key, email:_ } => key,
            _ => panic!("vcs account is not for github but {}", &config.vcs)
        };
        let user_and_repo = (self as &dyn vcs::VCS).user_and_repo()?;
        let response = self.shell.exec(shell::args![
            "curl", "--fail", "-sS", format!(
                "https://api.github.com/repos/{}/{}/releases/tags/{}",
                user_and_repo.0, user_and_repo.1, target_ref.0
            ), "-H", shell::fmtargs!("Authorization: token {}", token)
        ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
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
    fn determine_release_target(&self, ref_name: &str, is_branch: bool) -> Option<String> {
        let config = self.config.borrow();
        for (k,v) in &config.release_targets {
            if is_branch && !v.is_branch() {
                continue;
            }
            if !is_branch && !v.is_tag() {
                continue;
            }
            for path in v.paths() {
                let re = Pattern::new(&path.resolve()).unwrap();
                match re.matches(&ref_name) {
                    true => return Some(k.to_string()),
                    false => {}, 
                }
            }
        }
        None
    }
    fn url_from_pull_ref(&self, ref_name: &str) -> String {
        let user_and_repo = (self as &dyn vcs::VCS).user_and_repo().unwrap();
        format!("https://github.com/{}/{}/{}", user_and_repo.0, user_and_repo.1, ref_name)
    }
    fn pr_data_from_ref_path(&self, ref_path: &str, json_path: &str) ->Result<String, Box<dyn Error>> {
        if let config::vcs::Account::Github{ key, .. } = &self.config.borrow().vcs {
            let pr_url = self.url_from_pull_ref(ref_path);
            let api_url = format!(
                "https://api.github.com/repos/{pr_part}",
                pr_part = &pr_url[19..].replace("/pull/", "/pulls/")
            );
            let output = self.shell.exec(shell::args![
                "curl", "-s", "-H", shell::fmtargs!("Authorization: token {}", key), 
                "-H", "Accept: application/vnd.github.v3+json", api_url
            ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
            Ok(jsonpath(&output, json_path)
                .expect(&format!("malform pulls response: {:?} for json_path {}", &output, json_path))
                .unwrap_or("".to_string()))    
        } else {
            panic!("vcs account is not for github {}", &self.config.borrow().vcs)
        }
    }
}

impl<GIT: git::GitFeatures<S>, S: shell::Shell> vcs::VCS for Github<GIT, S> {
    fn new(config: &config::Container) -> Result<Github<GIT,S>, Box<dyn Error>> {
        if let config::vcs::Account::Github{ account, key, email } = &config.borrow().vcs {
            return Ok(Github {
                config: config.clone(),
                diff: vec!(),
                shell: S::new(config),
                git: GIT::new(&account, &email, &key, S::new(config))
            });
        } 
        return Err(Box::new(vcs::VCSError {
            cause: format!("should have github config but {}", config.borrow().vcs)
        }))
    }
    fn release_target(&self) -> Option<String> {        
        match self.git.current_ref().unwrap() {
            (vcs::RefType::Pull, ref_name) => {
                let base = self.pr_data_from_ref_path(&ref_name, "$.base.ref").unwrap();
                self.determine_release_target(&base, true)
            },
            (vcs::RefType::Tag, ref_name) => self.determine_release_target(&ref_name, false),
            (vcs::RefType::Branch|vcs::RefType::Remote, ref_name) => self.determine_release_target(&ref_name, true),
            (vcs::RefType::Commit, _) => None
        }
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
                    config::vcs::Account::Github{ account:_, key, email:_ } => key,
                    _ => panic!("vcs account is not for github {}", &config.vcs)
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
                self.shell.exec(shell::args![
                    "curl", "-sS", format!(
                        "https://api.github.com/repos/{}/{}/releases", 
                        user_and_repo.0, user_and_repo.1
                    ), 
                    "-H", shell::fmtargs!("Authorization: token {}", token), 
                    "-d", serde_json::to_string(&options)?
                ], shell::no_env(), shell::no_cwd(), &shell::no_capture())?             
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
            config::vcs::Account::Github{ account:_, key, email:_ } => key,
            _ => panic!("vcs account is not for github {}", &config.vcs)
        };
        let content_type = match opts.get("content-type") {
            Some(v) => v.as_str().unwrap_or("application/octet-stream").to_string(),
            None => "application/octet-stream".to_string()
        };
        let response = self.shell.exec(shell::args![
            "curl", "-sS", upload_url_base.replace("uploads.github.com", "api.github.com"),
            "-H", shell::fmtargs!("Authorization: token {}", token)
        ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
        match jsonpath(&response, &format!("$.[?(@.name=='{}')]", asset_name))? {
            Some(v) => match opts.get("replace") {
                Some(_) => {
                    // delete old asset
                    let delete_url = self.get_value_from_json_object(&v, "url")?;
                    self.shell.exec(shell::args![
                        "curl", delete_url, "-X", "DELETE", "-H", shell::fmtargs!("Authorization: token {}", token)
                    ], shell::no_env(), shell::no_cwd(), &shell::no_capture())?;
                },
                // nothing to do, return browser_download_url
                None => return self.get_value_from_json_object(&v, "browser_download_url")
            },
            None => log::debug!("no asset with name {}, proceed to upload", asset_name),
        };
        let response = self.shell.exec(shell::args![
            "curl", "-sS", upload_url, "-H", shell::fmtargs!("Authorization: token {}", token),
            "-H", format!("Content-Type: {}", content_type),
            "--data-binary", format!("@{}", asset_file_path)
        ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
        self.get_value_from_json_object(&response, "browser_download_url")
    }
    fn rebase_with_remote_counterpart(&self, branch: &str) -> Result<(), Box<dyn Error>> {
        self.git.rebase_with_remote_counterpart(&self.pushable_remote_url()?, branch)
    }
    fn current_ref(&self) -> Result<(vcs::RefType, String), Box<dyn Error>> {
        self.git.current_ref()
    }
    fn delete_branch(&self, ref_type: vcs::RefType, ref_path: &str) -> Result<(), Box<dyn Error>> {
        self.with_remote_for_push(|remote_url| {
            self.git.delete_branch(remote_url, ref_type, ref_path)
        })
    }
    fn fetch_branch(&self, branch_name: &str) -> Result<(), Box<dyn Error>> {
        self.with_remote_for_push(|remote_url| {
            self.git.fetch_branch(remote_url, branch_name)
        })
    }
    fn fetch_object(&self, hash: &str, ref_name: &str, depth: Option<usize>) -> Result<(), Box<dyn Error>> {
        self.with_remote_for_push(|remote_url| {
            self.git.fetch_object(remote_url, hash, ref_name, depth)
        })
    }
    fn squash_branch(&self, n: usize) -> Result<(), Box<dyn Error>> {
        self.git.squash_branch(n)
    }
    fn checkout(&self, commit: &str, branch_name: Option<&str>) -> Result<(), Box<dyn Error>> {
        self.git.checkout(commit, branch_name)
    }
    fn commit_hash(&self, expr: Option<&str>) -> Result<String, Box<dyn Error>> {
        self.git.commit_hash(expr)
    }
    fn repository_root(&self) -> Result<String, Box<dyn Error>> {
        self.git.repository_root()
    }
    fn pr(
        &self, title: &str, head_branch: &str, base_branch: &str, options: &HashMap<&str, &str>
    ) -> Result<(), Box<dyn Error>> {
        if let config::vcs::Account::Github{ key, .. } = &self.config.borrow().vcs {
            let user_and_repo = (self as &dyn vcs::VCS).user_and_repo().unwrap();
            let mut body = hashmap!{
                "title" => title, "owner" => &user_and_repo.0, 
                "repo" => &user_and_repo.1,
                "head" => head_branch, "base" => base_branch                
            };
            for (k, v) in options {
                if *k != "labels" && *k != "assignees"{
                    body.insert(k, v);
                }
            }
            let response = self.shell.exec(shell::args![
                "curl", "-H", shell::fmtargs!("Authorization: token {}", key), 
                "-H", "Accept: application/vnd.github.v3+json", 
                format!(
                    "https://api.github.com/repos/{}/{}/pulls", 
                    user_and_repo.0, user_and_repo.1
                ),
                "-d", serde_json::to_string(&body)?
                ], shell::no_env(), shell::no_cwd(), &shell::capture()
            )?;
            let issues_api_url = jsonpath(&response, "$.issue_url")
                .expect(&format!("malform pulls response: {:?}", &response))
                .expect(&format!("no issue_url in response: {:?}", &response));
            match options.get("labels") {
                Some(labels) => {
                    log::debug!("attach labels({}) to PR via {}", labels, issues_api_url);
                    self.shell.exec(shell::args![
                        "curl", "-H", shell::fmtargs!("Authorization: token {}", key), 
                        "-H", "Accept: application/vnd.github.v3+json", format!("{}/labels", issues_api_url),
                        "-d", format!(r#"{{"labels":{}}}"#, labels)
                    ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
                },
                None => {}
            };
            match options.get("assignees") {
                Some(assignees) => {
                    log::debug!("assign accounts({}) to PR via {}", assignees, issues_api_url);
                    self.shell.exec(shell::args![
                        "curl", "-H", shell::fmtargs!("Authorization: token {}", key), 
                        "-H", "Accept: application/vnd.github.v3+json", format!("{}/assignees", issues_api_url),
                        "-d", format!(r#"{{"assignees":{}}}"#, assignees)
                    ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
                },
                None => {}
            };
        } else {
            panic!("vcs is not github: {}", self.config.borrow().vcs);
        }
        Ok(())
    }
    fn pr_url_from_env(&self) -> Result<Option<String>, Box<dyn Error>> {
        match self.git.current_ref()? {
            (vcs::RefType::Branch|vcs::RefType::Remote, _) => Ok(None),
            (vcs::RefType::Pull, ref_name) => Ok(Some(self.url_from_pull_ref(&ref_name))),
            (vcs::RefType::Tag, _) => Ok(None),
            (vcs::RefType::Commit, _) => Ok(None)
        }
    }
    fn user_and_repo(&self) -> Result<(String, String), Box<dyn Error>> {
        let remote_url = self.git.remote_url(None)?;
        let re = regex::Regex::new(r"[^:]+[:/]([^/\.]+)/([^/\.]+)").unwrap();
        let user_and_repo = match re.captures(&remote_url) {
            Some(c) => (
                c.get(1).map_or("".to_string(), |m| m.as_str().to_string()), 
                c.get(2).map_or("".to_string(), |m| m.as_str().to_string())
            ),
            None => return escalate!(Box::new(vcs::VCSError {
                cause: format!("invalid remote origin url: {}", remote_url)
            }))
        };
        Ok(user_and_repo)
    }
    fn pick_ref(&self, ref_spec: &str) -> Result<(), Box<dyn Error>> {
        self.git.cherry_pick(ref_spec)
    }
    fn push_branch(
        &self, local_ref: &str, remote_branch: &str, option: &HashMap<&str, &str>
    ) -> Result<(), Box<dyn Error>> {
        self.with_remote_for_push(|remote_url| {
            self.git.push_branch(remote_url, local_ref, remote_branch, option)
        })
    }
    fn push_diff(
        &self, branch: &str, msg: &str, patterns: &Vec<&str>, options: &HashMap<&str, &str>
    ) -> Result<bool, Box<dyn Error>> {
        self.with_remote_for_push(|remote_url| {
            self.git.push_diff(remote_url, branch, msg, patterns, options)
        })
    }
    fn make_diff(&self) -> Result<String, Box<dyn Error>> {
        let diff = match self.git.current_ref()? {
            (vcs::RefType::Branch|vcs::RefType::Remote|vcs::RefType::Pull, _) => {
                self.git.diff_paths("HEAD^")?
            },
            (vcs::RefType::Tag, ref_name) => {
                let tags = self.with_remote_for_push(|remote_url| {
                    self.git.tags(remote_url)
                })?;
                let index = tags.iter().position(|tag| 
                    tag[1].replace("refs/tags/", "") == ref_name.as_str()
                ).ok_or(
                    make_escalation!(Box::new(vcs::VCSError {
                        cause: format!("tag {} does not found for list {:?}", ref_name, tags)
                    }))
                )?;
                if index == 0 {
                    // this is first tag, so treat as it changes everyhing
                    "*".to_string()
                } else {
                    let (src, dst) = (
                        &tags[index - 1][1].replace("^{}", ""), 
                        &tags[index][1].replace("^{}", "")
                    );
                    // fetch previous tag that does not usually fetched
                    self.fetch_object(&tags[index - 1][0], src, Some(1))?;
                    // diffing with previous tag
                    self.git.diff_paths(&format!("{}..{}", src, dst))?
                }
            },
            (vcs::RefType::Commit, ref_name) => {
                match self.git.diff_paths("HEAD^") {
                    Ok(v) => v,
                    Err(e) => return escalate!(Box::new(vcs::VCSError {
                        cause: format!(
                            "current head does not branch or tag {} and cannot get diff with HEAD^ {:?}", 
                            ref_name, e
                        )
                    }))
                }
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
