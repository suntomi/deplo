use std::panic;
use std::error::Error;
use std::collections::HashMap;
use std::path::Path;
use std::result::Result;
use std::time::{SystemTime, UNIX_EPOCH};
use std::rc::Rc;
use std::cell::{RefCell};

use chrono::{DateTime, Utc};
use glob::Pattern;
use maplit::hashmap;
use regex;
use serde::Deserialize;
use serde::Serialize;
use serde_json::{Value as JsonValue};
use jsonwebtoken::{encode, Header, EncodingKey, Algorithm};

use crate::config;
use crate::util::merge_hashmap;
use crate::vcs::{self, VCS};
use crate::shell;
use crate::util::{escalate,make_escalation,jsonpath,str_to_json,json_to_strmap};
use crate::config::value::Value;

use super::git;

#[derive(Serialize, Deserialize)]
struct Claims {
    iat: u64,  // issued at
    exp: u64,  // expiration (max 10 minutes)
    iss: String, // App ID
}

struct AppTokenGeneratorInner<S: shell::Shell> {
    app_id: config::Value,
    private_key: config::Value,
    shell: S,
    user_and_repo: (String, String),
    token: config::Value,
    token_expires_at: SystemTime,
}
#[derive(Clone)]
pub struct AppTokenGenerator<S: shell::Shell> {
    inner: Rc<RefCell<AppTokenGeneratorInner<S>>>,
}
impl<S: shell::Shell> AppTokenGenerator<S> {
    pub fn new(
        shell: S, app_id: config::Value, private_key: config::Value,
        user_and_repo: (String, String)
    ) -> Self {
        AppTokenGenerator {
            inner: Rc::new(RefCell::new(AppTokenGeneratorInner {
                app_id,
                private_key,
                shell,
                user_and_repo,
                token: config::Value::new(""),
                token_expires_at: UNIX_EPOCH,
            }))
        }
    }
    pub fn generate(&self) -> Result<String, Box<dyn Error>> {
        self.inner.borrow_mut().generate()
    }
    pub fn app_id(&self) -> String {
        self.inner.borrow().app_id.resolve()
    }
}
impl<S: shell::Shell> AppTokenGeneratorInner<S> {
    fn generate(&mut self) -> Result<String, Box<dyn Error>> {
        if self.token_expires_at > SystemTime::now() {
            return Ok(self.token.resolve());
        }
        let (token, token_expires_at) = Self::generate_installation_token(
            &self.shell,
            &self.app_id.resolve(),
            &self.private_key.resolve(),
            &self.user_and_repo
        )?;
        self.token = config::Value::new(&token);
        self.token_expires_at = DateTime::parse_from_rfc3339(&token_expires_at)?
            .with_timezone(&Utc).into();
        Ok(self.token.resolve())
    }
    fn generate_jwt(app_id: &str, private_key: &str) -> Result<String, Box<dyn Error>> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        
        let claims = Claims {
            iat: now - 60,  // 60秒前（時刻のずれ対策）
            exp: now + 600, // 10分後（最大値）
            iss: app_id.to_string(),
        };
        
        let key = EncodingKey::from_rsa_pem(private_key.as_bytes())?;
        let token = encode(&Header::new(Algorithm::RS256), &claims, &key)?;
        
        Ok(token)
    }

    fn get_installation_id(
        shell: &impl shell::Shell, jwt: &str, user_and_repo: &(String, String)
    ) -> Result<String, Box<dyn Error>> {
        let response = shell.exec(shell::args![
            "curl", "-sS", "-H", shell::fmtargs!("Authorization: Bearer {}", jwt),
            "-H", "Accept: application/vnd.github.v3+json",
            format!("https://api.github.com/repos/{}/{}/installation", user_and_repo.0, user_and_repo.1)
        ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
        let installation_id = jsonpath(&response, "$.id")
            .expect(&format!("malform installation id response: {:?}", &response))
            .expect(&format!("no installation id in response: {:?}", &response));
        Ok(installation_id)
    }

    fn generate_installation_token(
        shell: &impl shell::Shell, app_id: &str, private_key: &str, user_and_repo: &(String, String)
    ) -> Result<(String, String), Box<dyn Error>> {
        let jwt = Self::generate_jwt(app_id, private_key)?;
        let installation_id = Self::get_installation_id(shell, &jwt, user_and_repo)?;
        let response = shell.exec(shell::args![
            "curl", "-sS", "-X", "POST", format!(
                "https://api.github.com/app/installations/{}/access_tokens",
                installation_id
            ),
            "-H", shell::fmtargs!("Authorization: Bearer {}", jwt),
            "-H", "Accept: application/vnd.github.v3+json"
        ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
        let token = jsonpath(&response, "$.token")
            .expect(&format!("malform installation token response: {:?}", &response))
            .expect(&format!("no token in response: {:?}", &response));
        let token_expires_at = jsonpath(&response, "$.expires_at")
            .expect(&format!("malform installation token response: {:?}", &response))
            .expect(&format!("no expires_at in response: {:?}", &response));
        Ok((token, token_expires_at))
    }
}


#[derive(Serialize, Deserialize)]
struct MergeResult {
    merged: bool,
    message: String,
    sha: String,
}

pub struct Github<GIT: git::GitFeatures<S> = git::Git, S: shell::Shell = shell::Default> {
    pub config: config::Container,
    pub git: GIT,
    pub shell: S,
    pub app_token_generator: Option<AppTokenGenerator<S>>,
    pub diff: Vec<String>
}

impl<GIT: git::GitFeatures<S>, S: shell::Shell> Github<GIT, S> {
    fn pushable_remote_url(&self) -> Result<String, Box<dyn Error>> {
        let user_and_repo = (self as &dyn vcs::VCS).user_and_repo()?;
        Ok(format!("https://github.com/{}/{}", user_and_repo.0, user_and_repo.1))
    }
    fn enable_auto_merge_pr(
        &self, url: &str, options: &HashMap<&str, &str>
    ) -> Result<bool, Box<dyn Error>> {
        if let config::vcs::Account::Github{ key, .. } = &self.config.borrow().vcs {
            let user_and_repo = (self as &dyn vcs::VCS).user_and_repo().unwrap();
            let pr_num = url.split('/').last().unwrap_or("");
            
            // First, get PR ID using GraphQL query
            let query_pr_id = format!(r#"
{{"query": "query {{ repository(owner: \"{}\", name: \"{}\") {{ pullRequest(number: {}) {{ id }} }} }}"}}"#,
                user_and_repo.0, user_and_repo.1, pr_num);
            
            let pr_response = self.shell.exec(shell::args![
                "curl", "-sS", "-X", "POST", "https://api.github.com/graphql",
                "-H", shell::fmtargs!("Authorization: Bearer {}", key),
                "-H", "Content-Type: application/json",
                "-d", query_pr_id
            ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
            
            let pr_data: serde_json::Value = serde_json::from_str(&pr_response)?;
            let pr_id = pr_data["data"]["repository"]["pullRequest"]["id"]
                .as_str()
                .ok_or("Failed to get PR ID")?;
            
            // Enable auto-merge using GraphQL mutation
            let merge_method = match options.get("merge_method") {
                Some(method) => match method.to_lowercase().as_str() {
                    "squash" => "SQUASH",
                    "rebase" => "REBASE", 
                    _ => "MERGE"
                },
                None => "MERGE"
            };
            let commit_headline = options.get("commit_title").unwrap_or(&"Auto-merge by deplo");
            let commit_body = options.get("commit_message").unwrap_or(&"");
            
            let mutation = format!(r#"
{{"query": "mutation {{ enablePullRequestAutoMerge(input: {{ pullRequestId: \"{}\", mergeMethod: {}, commitHeadline: \"{}\", commitBody: \"{}\" }}) {{ pullRequest {{ autoMergeRequest {{ enabledAt }} }} }} }}"}}"#, 
                pr_id, merge_method, commit_headline, commit_body);
            
            let response = self.shell.exec(shell::args![
                "curl", "-sS", "-X", "POST", "https://api.github.com/graphql",
                "-H", shell::fmtargs!("Authorization: Bearer {}", key),
                "-H", "Content-Type: application/json",
                "-d", mutation
            ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
            
            let result: serde_json::Value = serde_json::from_str(&response)?;
            if let Some(errors) = result.get("errors") {
                let default_strval = serde_json::Value::String("".to_string());
                // check if errors has entry its path is enablePullRequestAutoMerge and error type is UNPROCESSABLE
                for e in errors.as_array().unwrap_or(&vec![]) {
                    if e.get("type").unwrap_or(&default_strval) == &"UNPROCESSABLE" {
                        for ee in e.get("path").unwrap_or(
                            &serde_json::Value::Array(vec![])
                        ).as_array().unwrap_or(&vec![]) {
                            log::info!("error path: {:?}", ee);
                            if ee == &"enablePullRequestAutoMerge" {
                                // already clean state. continue to normal merge
                                log::info!("may be already clean state. continue to normal merge: {:?}", e.get("message"));
                                return Ok(false);
                            }
                        }
                    }
                }
                return escalate!(Box::new(vcs::VCSError {
                    cause: format!("Failed to enable auto-merge: {}", errors)
                }));
            }
            
            log::info!("Auto-merge enabled for PR {}", url);
        } else {
            panic!("vcs is not github: {}", self.config.borrow().vcs);
        }
        Ok(true)
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
        let user_and_repo = self.user_and_repo()?;
        let (token, auth_type) = (self as &dyn vcs::VCS).get_token()?;
        let response = self.shell.exec(shell::args![
            "curl", "--fail", "-sS", format!(
                "https://api.github.com/repos/{}/{}/releases/tags/{}",
                user_and_repo.0, user_and_repo.1, target_ref.0
            ), "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token)
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
        let (token, auth_type) = (self as &dyn vcs::VCS).get_token()?;
        let pr_url = self.url_from_pull_ref(ref_path);
        let api_url = format!(
            "https://api.github.com/repos/{pr_part}",
            pr_part = &pr_url[19..].replace("/pull/", "/pulls/")
        );
        let output = self.shell.exec(shell::args![
            "curl", "-s", "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token), 
            "-H", "Accept: application/vnd.github.v3+json", api_url
        ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
        Ok(jsonpath(&output, json_path)
            .expect(&format!("malform pulls response: {:?} for json_path {}", &output, json_path))
            .unwrap_or("".to_string()))    
    }
}

impl<GIT: git::GitFeatures<S>, S: shell::Shell> vcs::VCS for Github<GIT, S> {
    fn new(config: &config::Container) -> Result<Github<GIT,S>, Box<dyn Error>> {
        if let config::vcs::Account::Github{ account, key, email } = &config.borrow().vcs {
            return Ok(Github {
                config: config.clone(),
                diff: vec!(),
                shell: S::new(config),
                app_token_generator: None,
                git: GIT::from_pat(&account, &email, &key,S::new(config))
            });
        } else if let config::vcs::Account::GithubApp{ app_id, pkey, local_fallback } = &config.borrow().vcs {
            // Use local_fallback when running locally and fallback is configured
            if !config::Config::is_running_on_ci() {
                if let Some(fallback) = local_fallback {
                    return Ok(Github {
                        config: config.clone(),
                        diff: vec!(),
                        shell: S::new(config),
                        app_token_generator: None,
                        git: GIT::from_pat(&fallback.account, &fallback.email, &fallback.key, S::new(config))
                    });
                }
            }
            let app_token_generator = AppTokenGenerator::<S>::new(
                S::new(config), app_id.clone(), pkey.clone(),
                ("".to_string(), "".to_string())
            );
            let git = GIT::from_app(
                AppTokenGenerator { inner: app_token_generator.inner.clone() },
                S::new(config)
            );
            app_token_generator.inner.borrow_mut().user_and_repo = git.user_and_repo()?;
            return Ok(Github {
                config: config.clone(),
                diff: vec!(),
                shell: S::new(config),
                app_token_generator: Some(app_token_generator),
                git: git
            });
        } else {
            return Err(Box::new(vcs::VCSError {
                cause: format!("should have github config but {}", config.borrow().vcs)
            }))
        }
    }
    fn get_token(&self) -> Result<(Value, &str), Box<dyn Error>> {
        let config = self.config.borrow();
        if let config::vcs::Account::Github{ key, .. } = &self.config.borrow().vcs {
            Ok((key.clone(), "token"))
        } else if let config::vcs::Account::GithubApp{ local_fallback, .. } = &self.config.borrow().vcs {
            // Use local_fallback when running locally and fallback is configured
            if !config::Config::is_running_on_ci() {
                if let Some(fallback) = local_fallback {
                    return Ok((fallback.key.clone(), "token"));
                }
            }
            Ok((Value::new(&self.app_token_generator.as_ref().unwrap().generate()?), "Bearer"))
        } else {
            return Err(Box::new(vcs::VCSError {
                cause: format!("should have github config but {}", config.vcs)
            }))
        }
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
                let user_and_repo = self.user_and_repo()?;
                let (token, auth_type) = self.get_token()?;
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
                    "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token), 
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
        let (token, auth_type) = self.get_token()?;
        let content_type = match opts.get("content-type") {
            Some(v) => v.as_str().unwrap_or("application/octet-stream").to_string(),
            None => "application/octet-stream".to_string()
        };
        let response = self.shell.exec(shell::args![
            "curl", "-sS", upload_url_base.replace("uploads.github.com", "api.github.com"),
            "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token)
        ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
        match jsonpath(&response, &format!("$.[?(@.name=='{}')]", asset_name))? {
            Some(v) => match opts.get("replace") {
                Some(_) => {
                    // delete old asset
                    let delete_url = self.get_value_from_json_object(&v, "url")?;
                    self.shell.exec(shell::args![
                        "curl", delete_url, "-X", "DELETE", "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token)
                    ], shell::no_env(), shell::no_cwd(), &shell::no_capture())?;
                },
                // nothing to do, return browser_download_url
                None => return self.get_value_from_json_object(&v, "browser_download_url")
            },
            None => log::debug!("no asset with name {}, proceed to upload", asset_name),
        };
        let response = self.shell.exec(shell::args![
            "curl", "-sS", upload_url, "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token),
            "-H", format!("Content-Type: {}", content_type),
            "--data-binary", format!("@{}", asset_file_path)
        ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
        self.get_value_from_json_object(&response, "browser_download_url")
    }
    fn rebase_with_remote_counterpart(&self, branch: &str) -> Result<(), Box<dyn Error>> {
        self.git.rebase_with_remote_counterpart(&self.pushable_remote_url()?, branch)
    }
    fn search_remote_ref(&self, commit: &str) -> Result<Option<String>, Box<dyn Error>> {
        self.git.search_remote_ref(commit)
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
        let user_and_repo = self.user_and_repo()?;
        let (token, auth_type) = self.get_token()?;
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
            "curl", "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token), 
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
                    "curl", "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token), 
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
                    "curl", "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token), 
                    "-H", "Accept: application/vnd.github.v3+json", format!("{}/assignees", issues_api_url),
                    "-d", format!(r#"{{"assignees":{}}}"#, assignees)
                ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
            },
            None => {}
        };
        Ok(())
    }
    fn merge_pr(
        &self, url: &str, opts: &JsonValue
    ) -> Result<(), Box<dyn Error>> {
        let optmap_src = json_to_strmap(&opts);
        let options = optmap_src.iter().map(|(k, v)| (*k, v.as_str())).collect::<HashMap<_, _>>();        
        // merge pr with github api
        let user_and_repo = self.user_and_repo()?;
        let (token, auth_type) = self.get_token()?;
        let pr_num = url.split('/').last().unwrap_or("");
        if let Some(message) = options.get("message") {
            let api_url = format!(
                "https://api.github.com/repos/{}/{}/issues/{}/comments",
                user_and_repo.0, user_and_repo.1, pr_num
            );
            self.shell.exec(shell::args![
                "curl", "-sS", "-X", "POST", api_url,
                "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token),
                "-H", "Accept: application/vnd.github.v3+json",
                "-d", format!(r#"{{"body":"{}"}}"#, message)
            ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
        }            
        // if options are set, approve first
        if options.get("approve").unwrap_or(&"true") == &"true" {
            let api_url = format!(
                "https://api.github.com/repos/{}/{}/pulls/{}/reviews",
                user_and_repo.0, user_and_repo.1, pr_num
            );
            self.shell.exec(shell::args![
                "curl", "-sS", "-X", "POST", api_url,
                "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token),
                "-H", "Accept: application/vnd.github.v3+json",
                "-d", r#"{"event":"APPROVE"}"#
            ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
        }
        // Check if auto_merge option is requested
        if options.get("auto_merge").unwrap_or(&"false") == &"true" {
            if self.enable_auto_merge_pr(url, &options)? {
                log::info!("successfully auto merged PR {}", url);
                return Ok(()); // auto-merge enabled and not clean status. PR will wait for condition met
            }
        }
        let api_url = format!(
            "https://api.github.com/repos/{}/{}/pulls/{}/merge",
            user_and_repo.0, user_and_repo.1, pr_num
        );
        let default_message = format!("deplo version: {}, commit: {}", 
            config::DEPLO_VERSION, config::DEPLO_GIT_HASH);
        let default_commit_title = format!("PR #{} merged by deplo", pr_num);
        let default_body = hashmap!{
            "merge_method" => "merge", "commit_title" => &default_commit_title,
            "commit_message" => default_message.as_str()
        };
        let body = merge_hashmap(&default_body, &options);
        let response_text = self.shell.exec(shell::args![
            "curl", "-sS", "-X", "PUT", api_url,
            "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token),
            "-H", "Accept: application/vnd.github.v3+json",
            "-d", serde_json::to_string(&body)?
        ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
        let response = serde_json::from_str::<MergeResult>(&response_text)?;
        if !response.merged {
            return escalate!(Box::new(vcs::VCSError {
                cause: format!("failed to merge PR {}: {}", url, response.message)
            }));
        } else {
            log::info!("successfully merged PR {}: sha={}", url, response.sha);
        }
        Ok(())
    }
    fn close_pr(
        // options has merge_method, commit_message, comit_title, sha
        &self, url: &str, opts: &JsonValue
    ) -> Result<(), Box<dyn Error>> {
        let user_and_repo = self.user_and_repo()?;
        let (token, auth_type) = self.get_token()?;
        let pr_num = url.split('/').last().unwrap_or("");
        let api_url = format!(
            "https://api.github.com/repos/{}/{}/pulls/{}",
            user_and_repo.0, user_and_repo.1, pr_num
        );
        let optmap_src = json_to_strmap(&opts);
        let options = optmap_src.iter().map(|(k, v)| (*k, v.as_str())).collect::<HashMap<_, _>>();
        // if options["message"] is set, use it as comment to pull request, then close it
        if let Some(message) = options.get("message") {
            let comment_url = format!("{}/comments", api_url).replace("pulls", "issues");
            self.shell.exec(shell::args![
                "curl", "-sS", "-X", "POST", comment_url,
                "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token),
                "-H", "Accept: application/vnd.github.v3+json",
                "-d", format!(r#"{{"body":"{}"}}"#, message)
            ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
        }
        self.shell.exec(shell::args![
            "curl", "-sS", "-X", "PATCH", api_url,
            "-H", shell::fmtargs!("Authorization: {} {}", auth_type, &token),
            "-H", "Accept: application/vnd.github.v3+json",
            "-d", r#"{"state":"closed"}"#
        ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
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
       self.git.user_and_repo()
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
