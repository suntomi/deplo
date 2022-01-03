use std::error::Error;
use std::collections::HashMap;

#[cfg(feature="git2")]
use git2::{Repository,RepositoryOpenFlags};
// I love rust in many way, but I want #if~#else~#endif feature to avoid such useless code.
// TODO: more concise separation for feature flag git2's on/off.
// impl GitFeature itself should be separated like CrateRepositoryInspector and ShellRepositoryInspector
// and use pub type RepositoryInspector = CrateRepositoryInspector|ShellRepositoryInspector according to the flag on/off.
#[cfg(not(feature="git2"))]
pub struct StubRepository {}
#[cfg(not(feature="git2"))]
impl StubRepository {
    fn find_remote(&self, _: &str) -> Result<StubRemote, Box<dyn Error>> {
        Ok(StubRemote{})
    }
}
#[cfg(not(feature="git2"))]
pub struct StubRemote {}
#[cfg(not(feature="git2"))]
impl StubRemote {
    fn url(&self) -> Option<&str> {
        Some("")
    }
}

use maplit::hashmap;

use crate::config;
use crate::shell;
use crate::util::{defer, escalate, jsonpath};
use crate::vcs;



// because defer uses Drop trait behaviour, this cannot be de-duped as function
macro_rules! setup_remote {
    ($git:expr, $url:expr) => {
        $git.shell.exec(&vec!(
            "git", "remote", "add", "latest", $url
        ), shell::no_env(), shell::no_cwd(), false)?;
        // defered removal of latest
        defer!(
            $git.shell.exec(&vec!(
                "git", "remote", "remove", "latest"
            ), shell::no_env(), shell::no_cwd(), false).unwrap();
        );
    };
}

pub struct Git<S: shell::Shell = shell::Default> {
    config: config::Container,
    username: String,
    email: String,
    shell: S,
    #[cfg(feature="git2")]
    repo: Repository,
    #[cfg(not(feature="git2"))]
    repo: StubRepository,
}

pub trait GitFeatures {
    fn new(username: &str, email: &str, config: &config::Container) -> Self;
    fn current_ref(&self) -> Result<(vcs::RefType, String), Box<dyn Error>>;
    fn current_branch(&self) -> Result<(String, bool), Box<dyn Error>>;
    fn commit_hash(&self, expr: Option<&str>) -> Result<String, Box<dyn Error>>;
    fn remote_origin(&self) -> Result<String, Box<dyn Error>>;
    fn repository_root(&self) -> Result<String, Box<dyn Error>>;
    fn diff_paths(&self, expression: &str) -> Result<String, Box<dyn Error>>;
    fn rebase_with_remote_counterpart(
        &self, url: &str, remote_branch: &str
    ) -> Result<(), Box<dyn Error>>;
    fn push(
        &self, url: &str, remote_branch: &str, msg: &str, 
        patterns: &Vec<&str>, options: &HashMap<&str, &str>
    ) -> Result<bool, Box<dyn Error>>;
    fn tags(&self) -> Result<Vec<String>, Box<dyn Error>>;
}

pub trait GitHubFeatures {
    fn pr(
        &self, title: &str, head_branch: &str, base_branch: &str, options: &HashMap<&str, &str>
    ) -> Result<(), Box<dyn Error>>;  
    fn pr_data(
        &self, pr_url: &str, account: &str, token: &str, json_path: &str
    ) -> Result<String, Box<dyn Error>>;
}

impl<S: shell::Shell> Git<S> {
    fn setup_author(&self) -> Result<(), Box<dyn Error>> {
        log::debug!("git: setup {}/{}", self.email, self.username);
        self.shell.exec(&vec!(
            "git", "config", "--global", "user.email", &self.email
        ), shell::no_env(), shell::no_cwd(), false)?;
        self.shell.exec(&vec!(
            "git", "config", "--global", "user.name", &self.username
        ), shell::no_env(), shell::no_cwd(), false)?;
        Ok(())
    }
    fn hub_env(&self) -> Result<HashMap<String, String>, Box<dyn Error>> {
        let config = self.config.borrow();
        if let config::VCSConfig::Github{ email:_, account:_, key } = &config.vcs {
            Ok(hashmap!{
                "GITHUB_TOKEN".to_string() => key.to_string()
            })
        } else {
            Err(Box::new(config::ConfigError {
                cause: format!("should have github config, got: {}", config.vcs)
            }))
        }
    }
}

impl<S: shell::Shell> GitFeatures for Git<S> {
    fn new(username: &str, email: &str, config: &config::Container) -> Git<S> {
        #[cfg(feature="git2")]
        let cwd = std::env::current_dir().unwrap();
        return Git::<S> {
            config: config.clone(),
            username: username.to_string(),
            email: email.to_string(),
            shell: S::new(config),
            #[cfg(feature="git2")]
            repo: Repository::open_ext(
                match config.borrow().runtime.workdir {
                    Some(ref v) => std::path::Path::new(v),
                    None => cwd.as_path()
                },
                RepositoryOpenFlags::empty(), 
                &[std::env::var("HOME").unwrap()]
            ).unwrap(),
            #[cfg(not(feature="git2"))]
            repo: StubRepository {},
        }
    }
    fn current_ref(&self) -> Result<(vcs::RefType, String), Box<dyn Error>> {
        match self.shell.output_of(&vec!(
            "git", "describe" , "--all"
        ), shell::no_env(), shell::no_cwd()) {
            Ok(ref_path) => {
                if ref_path.starts_with("remotes/") {
                    // remote branch that does not have local counterpart
                    if ref_path[0..8].starts_with("pull") {
                        let pos = ref_path.rfind('/').expect(format!("invalid ref path: {}", ref_path).as_str());
                        return Ok((vcs::RefType::Pull, ref_path[8..(pos - 1)].to_string()));
                    } else {
                        return Ok((vcs::RefType::Branch, ref_path[8..].to_string()));
                    }
                } else if ref_path.starts_with("tags/") {
                    // tags
                    return Ok((vcs::RefType::Tag, ref_path[5..].to_string()));
                } else if ref_path.starts_with("heads/") {
                    // local branch
                    return Ok((vcs::RefType::Branch, ref_path[6..].to_string()));
                } else {
                    return Ok((vcs::RefType::Commit, self.commit_hash(None)?))
                }
            },
            Err(_) => {
                return Ok((vcs::RefType::Commit, self.commit_hash(None)?))
            }
        }
    }
    fn current_branch(&self) -> Result<(String, bool), Box<dyn Error>> {
        let (ref_type, ref_path) = self.current_ref()?;
        match ref_type {
            vcs::RefType::Pull => {
                // pull ref has actual head of branch exactly before merge commit
                Ok((self.shell.output_of(&vec!(
                    "git", "symbolic-ref" , "--short", "HEAD^"
                ), shell::no_env(), shell::no_cwd())?, true))
            },
            vcs::RefType::Tag => {
                Ok((ref_path, false))
            },
            vcs::RefType::Branch => {
                Ok((ref_path, true))
            },
            vcs::RefType::Commit => {
                escalate!(Box::new(vcs::VCSError {
                    cause: format!("not on a branch or tag, got: {}", ref_path)
                }))
            }
        }
    }
    fn commit_hash(&self, expr: Option<&str>) -> Result<String, Box<dyn Error>> {
        Ok(self.shell.output_of(&vec!(
            "git", "rev-parse" , expr.unwrap_or("HEAD")
        ), shell::no_env(), shell::no_cwd())?)
    }
    fn remote_origin(&self) -> Result<String, Box<dyn Error>> {
        if cfg!(feature="git2") {
            let origin = self.repo.find_remote("origin")?;
            Ok(origin.url().unwrap().to_string())
        } else {
            Ok(self.shell.output_of(&vec!(
                "git", "config", "--get", "remote.origin.url"
            ), shell::no_env(), shell::no_cwd())?)
        }
    }
    fn repository_root(&self) -> Result<String, Box<dyn Error>> {
        Ok(self.shell.output_of(&vec!(
            "git", "rev-parse", "--show-toplevel"
        ), shell::no_env(), shell::no_cwd())?)
    }
    fn diff_paths(&self, expression: &str) -> Result<String, Box<dyn Error>> {
        Ok(self.shell.output_of(
            &vec!("git", "--no-pager", "diff", "--name-only", expression),
            shell::no_env(), shell::no_cwd()
        )?)
    }
    fn tags(&self) -> Result<Vec<String>, Box<dyn Error>> {
        self.shell.exec(&vec!(
            "git", "pull", "--tags"
        ), shell::no_env(), shell::no_cwd(), false)?;
        Ok(self.shell.output_of(&vec!(
            "git", "tag"
        ), shell::no_env(), shell::no_cwd())?.split('\n').map(|s| s.to_string()).collect())
    }
    fn push(
        &self, url: &str, remote_branch: &str, msg: &str, 
        patterns: &Vec<&str>, options: &HashMap<&str, &str> 
    ) -> Result<bool, Box<dyn Error>> {
        let config = self.config.borrow();        
        let use_lfs = match options.get("use-lfs") {
            Some(v) => *v == "yes",
            None => false
        };
        if use_lfs {
            // this useless diffing is for making lfs tracked files refreshed.
            // otherwise if lfs tracked file is written, codes below seems to treat these write as git diff.
            // even if actually no change.
            // TODO_PATH: use Path to generate path of /dev/null
		    self.shell.exec(&vec!["git", "--no-pager", "diff"], shell::no_env(), shell::no_cwd(), false)?;
        }
		let mut changed = false;

		for pattern in patterns {
            self.shell.exec(&vec!("git", "add", "-N", pattern), shell::no_env(), shell::no_cwd(), false)?;
            let diff = self.shell.exec(&vec!("git", "add", "-n", pattern), shell::no_env(), shell::no_cwd(), true)?;
			if !diff.is_empty() {
                log::debug!("diff found for {} [{}]", pattern, diff);
                self.shell.exec(&vec!("git", "add", pattern), shell::no_env(), shell::no_cwd(), false)?;
				changed = true
            }
        }
		if !changed {
			log::debug!("skip push because no changes for provided pattern [{}]", patterns.join(" "));
			return Ok(false)
        } else {
			if use_lfs {
				self.shell.exec(
                    &vec!["git", "lfs", "fetch", "--all"], shell::no_env(), shell::no_cwd(), false
                )?;
            }
			self.shell.exec(&vec!("git", "commit", "-m", msg), shell::no_env(), shell::no_cwd(), false)?;
			log::debug!("commit done: [{}]", msg);
			match config.runtime_release_target() {
                Some(_) => {
                    setup_remote!(self, url);
                    let (b, is_branch) = self.current_branch()?;
                    if !is_branch {
                        log::debug!("skip push because current ref is not a branch {}", b);
                        return Ok(false)
                    }
                    // update remote counter part of the deploy branch again
                    // because sometimes other commits are made (eg. merging pull request, job updates metadata)
                    // here, $CI_BASE_BRANCH_NAME before colon means branch which name is $CI_BASE_BRANCH_NAME at remote `latest`
                    self.shell.exec(&vec!(
                        "git", "fetch", "--force", "latest", &format!("{}:remotes/latest/{}", b, b)
                    ), shell::no_env(), shell::no_cwd(), false)?;
                    // deploy branch: rebase CI branch with remotes `latest`. 
                    // because if other changes commit to the branch, below causes push error without rebasing it
                    self.shell.exec(&vec!(
                        "git", "rebase", &format!("remotes/latest/{}", b)
                    ), shell::no_env(), shell::no_cwd(), false)?;
                },
                None => {}
            }
			if use_lfs {
                self.shell.exec(&vec!("git", "lfs", "push", url, "--all"), shell::no_env(), shell::no_cwd(), false)?;
            }
            self.shell.exec(&vec!(
                "git", "push", "--no-verify", url, &format!("HEAD:{}", remote_branch)
            ), shell::no_env(), shell::no_cwd(), false)?;
			return Ok(true)
        }
    }
    fn rebase_with_remote_counterpart(
        &self, url: &str, remote_branch: &str
    ) -> Result<(), Box<dyn Error>> {
        // user and email
        self.setup_author()?;
        setup_remote!(self, url);
        // we cannot `git pull latest $remote_branch` here. eg. $remote_branch = master case on circleCI. 
        // sometimes latest/master and master diverged, and pull causes merge FETCH_HEAD into master.
        // then it raises error if no mail/user name specified, because these are diverged branches. 
        // (but we don't know how master and latest/master are diverged with cached .git of circleCI)

        // here, remote_branch before colon means branch which name is $remote_branch at remote `latest`
        // fetch remote counter part of the branch to temporary remote 'latest' for rebasing.
        self.shell.exec(&vec!(
            "git", "fetch", "--force", "latest", 
            &format!("{}:remotes/latest/{}", remote_branch, remote_branch)
        ), shell::no_env(), shell::no_cwd(), false)?;
        // rebase with fetched remote latest branch. it may change HEAD.
        self.shell.exec(&vec!(
            "git", "rebase", &format!("remotes/latest/{}", remote_branch)
        ), shell::no_env(), shell::no_cwd(), false)?;
        Ok(())
    }
}

impl<S: shell::Shell> GitHubFeatures for Git<S> {
    fn pr(
        &self, title: &str, head_branch: &str, base_branch: &str, options: &HashMap<&str, &str>
    ) -> Result<(), Box<dyn Error>> {
        match options.get("labels") {
            Some(l) => {
                self.shell.exec(&vec!(
                    "hub", "pull-request", "-f", "-m", title, 
                    "-h", head_branch, "-b", base_branch, "-l", l
                ), self.hub_env()?, shell::no_cwd(), false)?;
            },
            None => {
                self.shell.exec(&vec!(
                    "hub", "pull-request", "-f", "-m", title, 
                    "-h", head_branch, "-b", base_branch,
                ), self.hub_env()?, shell::no_cwd(), false)?;
            }
        }
        Ok(())
    }
    fn pr_data(
        &self, pr_url: &str, _account: &str, token: &str, json_path: &str
    ) -> Result<String, Box<dyn Error>> {
        let api_url = format!("https://api.github.com/repos/${pr_part}", pr_part = &pr_url[19..]);
        let output = self.shell.exec(&vec![
            "curl", "-s", "-H", &format!("Authorization: token {}", token), 
            "-H", "Accept: application/vnd.github.v3+json", &api_url
        ], shell::no_env(), shell::no_cwd(), true)?;
        Ok(jsonpath(&output, json_path)?.unwrap_or("".to_string()))
    }
}