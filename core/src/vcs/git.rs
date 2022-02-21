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
use crate::util::{defer, jsonpath};
use crate::vcs;



// because defer uses Drop trait behaviour, this cannot be de-duped as function
macro_rules! setup_remote {
    ($git:expr, $url:expr) => {
        $git.shell.exec(&vec!(
            "git", "remote", "add", "latest", $url
        ), shell::no_env(), shell::no_cwd(), &shell::no_capture())?;
        // defered removal of latest
        defer!(
            $git.shell.exec(&vec!(
                "git", "remote", "remove", "latest"
            ), shell::no_env(), shell::no_cwd(), &shell::no_capture()).unwrap();
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
    fn delete_branch(&self, url: &str, remote_branch: &str) -> Result<(), Box<dyn Error>>;
    fn commit_hash(&self, expr: Option<&str>) -> Result<String, Box<dyn Error>>;
    fn checkout(&self, commit: &str, branch_name: Option<&str>) -> Result<(), Box<dyn Error>>;
    fn remote_origin(&self) -> Result<String, Box<dyn Error>>;
    fn repository_root(&self) -> Result<String, Box<dyn Error>>;
    fn diff_paths(&self, expression: &str) -> Result<String, Box<dyn Error>>;
    fn rebase_with_remote_counterpart(
        &self, url: &str, remote_branch: &str
    ) -> Result<(), Box<dyn Error>>;
    fn cherry_pick(&self, branch: &str) -> Result<(), Box<dyn Error>>;
    fn push(
        &self, url: &str, remote_branch: &str, 
        local_ref: &str, option: &HashMap<&str, &str>
    ) -> Result<(), Box<dyn Error>>;
    fn push_diff(
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
    fn commit_env(&self) -> HashMap<String, String> {
        hashmap!{
            "GIT_COMMITTER_NAME".to_string() => self.username.to_string(),
            "GIT_AUTHOR_NAME".to_string() => self.username.to_string(),
            "GIT_COMMITTER_EMAIL".to_string() => self.email.to_string(),
            "GIT_AUTHOR_EMAIL".to_string() => self.email.to_string(),
        }
    }
    fn parse_ref_path(&self, ref_path: &str) -> Result<(vcs::RefType, String), Box<dyn Error>> {
        if ref_path.starts_with("remotes/") {
            // remote branch that does not have local counterpart
            if ref_path[8..].starts_with("pull") {
                let pos = ref_path.rfind('/').expect(format!("invalid ref path: {}", ref_path).as_str());
                return Ok((vcs::RefType::Pull, ref_path[8..pos].to_string()));
            } else {
                return Ok((vcs::RefType::Remote, ref_path[8..].to_string()));
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
                return self.parse_ref_path(&ref_path)
            },
            Err(_) => {
                return Ok((vcs::RefType::Commit, self.commit_hash(None)?))
            }
        }
    }
    fn delete_branch(&self, url: &str, remote_branch: &str) -> Result<(), Box<dyn Error>> {
        self.shell.exec(&vec!(
            "git", "push", url, "--delete", remote_branch
        ), shell::no_env(), shell::no_cwd(), &shell::no_capture())?;
        Ok(())
    }
    fn checkout(&self, commit: &str, branch_name: Option<&str>) -> Result<(), Box<dyn Error>> {
        match branch_name {
            Some(b) => self.shell.output_of(&vec!(
                "git", "checkout" , "-B", b, commit
            ), shell::no_env(), shell::no_cwd())?,
            None => self.shell.output_of(&vec!(
                "git", "checkout", commit
            ), shell::no_env(), shell::no_cwd())?
        };
        Ok(())
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
            "git", "fetch", "--tags"
        ), shell::no_env(), shell::no_cwd(), &shell::capture())?;
        Ok(self.shell.output_of(&vec!(
            "git", "tag"
        ), shell::no_env(), shell::no_cwd())?.split('\n').map(|s| s.to_string()).collect())
    }
    fn cherry_pick(&self, target: &str) -> Result<(), Box<dyn Error>> {
        self.shell.exec(&vec!(
            "git", "cherry-pick", &target
        ), shell::no_env(), shell::no_cwd(), &shell::capture())?;
        Ok(())        
    }
    fn push(
        &self, url: &str, remote_branch: &str, 
        local_ref: &str, options: &HashMap<&str, &str>
    ) -> Result<(), Box<dyn Error>> {
        let use_lfs = match options.get("lfs") {
            Some(v) => !v.is_empty(),
            None => false
        };
        self.rebase_with_remote_counterpart(url, remote_branch)?;
        if use_lfs {
            self.shell.exec(
                &vec!["git", "lfs", "fetch"], shell::no_env(), shell::no_cwd(), &shell::no_capture()
            )?;
            self.shell.exec(&vec!["git", "lfs", "push", url], self.commit_env(), shell::no_cwd(), &shell::no_capture())?;
        }
        self.shell.exec(&vec![
            "git", "push", "--no-verify", url, &format!("{}:{}", local_ref, remote_branch)
        ], self.commit_env(), shell::no_cwd(), &shell::no_capture())?;
        Ok(())
    }
    fn push_diff(
        &self, url: &str, remote_branch: &str, msg: &str, 
        patterns: &Vec<&str>, options: &HashMap<&str, &str> 
    ) -> Result<bool, Box<dyn Error>> {
        let config = self.config.borrow();        
        let use_lfs = match options.get("lfs") {
            Some(v) => !v.is_empty(),
            None => false
        };
        if use_lfs {
            // this useless diffing is for making lfs tracked files refreshed.
            // otherwise if lfs tracked file is written, codes below seems to treat these write as git diff.
            // even if actually no change.
		    self.shell.exec(&vec!["git", "--no-pager", "diff"], shell::no_env(), shell::no_cwd(), &shell::capture())?;
        }
		let mut changed = false;

		for pattern in patterns {
            self.shell.exec(&vec!("git", "add", "-N", pattern), shell::no_env(), shell::no_cwd(), &shell::no_capture())?;
            let diff = self.shell.exec(&vec!("git", "add", "-n", pattern), shell::no_env(), shell::no_cwd(), &shell::capture())?;
			if !diff.is_empty() {
                log::debug!("diff found for {} [{}]", pattern, diff);
                self.shell.exec(&vec!("git", "add", pattern), shell::no_env(), shell::no_cwd(), &shell::no_capture())?;
				changed = true
            }
        }
		if !changed {
			log::debug!("skip push because no changes for provided pattern [{}]", patterns.join(" "));
			return Ok(false)
        } else {
			if use_lfs {
				self.shell.exec(
                    &vec!["git", "lfs", "fetch"], shell::no_env(), shell::no_cwd(), &shell::no_capture()
                )?;
            }
			self.shell.exec(&vec!("git", "commit", "-m", msg), self.commit_env(), shell::no_cwd(), &shell::no_capture())?;
			log::debug!("commit done: [{}]", msg);
			if use_lfs {
                self.shell.exec(&vec!["git", "lfs", "push", url], self.commit_env(), shell::no_cwd(), &shell::no_capture())?;
            }
            self.shell.exec(&vec!(
                "git", "push", "--no-verify", url, &format!("HEAD:{}", remote_branch)
            ), self.commit_env(), shell::no_cwd(), &shell::no_capture())?;
			return Ok(true)
        }
    }
    fn rebase_with_remote_counterpart(
        &self, url: &str, remote_branch: &str
    ) -> Result<(), Box<dyn Error>> {
        // rebase to get latest remote branch, because sometimes latest/master and master diverged, 
        // and pull causes merge FETCH_HEAD into master.
        setup_remote!(self, url);

        // here, remote_branch before colon means branch which name is $remote_branch at remote `latest`
        // fetch remote counter part of the branch to temporary remote 'latest' for rebasing.
        self.shell.exec(&vec!(
            "git", "fetch", "--force", "latest", 
            &format!("{}:remotes/latest/{}", remote_branch, remote_branch)
        ), self.commit_env(), shell::no_cwd(), &shell::no_capture())?;
        // rebase with fetched remote latest branch. it may change HEAD.
        self.shell.exec(&vec!(
            "git", "rebase", &format!("remotes/latest/{}", remote_branch)
        ), self.commit_env(), shell::no_cwd(), &shell::no_capture())?;
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
                ), self.hub_env()?, shell::no_cwd(), &shell::no_capture())?;
            },
            None => {
                self.shell.exec(&vec!(
                    "hub", "pull-request", "-f", "-m", title, 
                    "-h", head_branch, "-b", base_branch,
                ), self.hub_env()?, shell::no_cwd(), &shell::no_capture())?;
            }
        }
        Ok(())
    }
    fn pr_data(
        &self, pr_url: &str, _account: &str, token: &str, json_path: &str
    ) -> Result<String, Box<dyn Error>> {
        let api_url = format!(
            "https://api.github.com/repos/{pr_part}",
            pr_part = &pr_url[19..].replace("/pull/", "/pulls/")
        );
        let output = self.shell.exec(&vec![
            "curl", "-s", "-H", &format!("Authorization: token {}", token), 
            "-H", "Accept: application/vnd.github.v3+json", &api_url
        ], shell::no_env(), shell::no_cwd(), &shell::capture())?;
        Ok(jsonpath(&output, json_path)?.unwrap_or("".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ref_path_test() {
        let git = Git::<shell::Default>::new("umegaya", "mail@address.com", &config::Config::dummy(None).unwrap());
        let path = "heads/main";
        let (ref_type, ref_name) = git.parse_ref_path(path).unwrap();
        assert_eq!(ref_type, vcs::RefType::Branch);
        assert_eq!(ref_name, "main");

        let path = "remotes/origin/umegaya/deplow";
        let (ref_type, ref_name) = git.parse_ref_path(path).unwrap();
        assert_eq!(ref_type, vcs::RefType::Remote);
        assert_eq!(ref_name, "origin/umegaya/deplow");

        let path = "remotes/pull/123/merge";
        let (ref_type, ref_name) = git.parse_ref_path(path).unwrap();
        assert_eq!(ref_type, vcs::RefType::Pull);
        assert_eq!(ref_name, "pull/123");

        let path = "tags/0.1.1";
        let (ref_type, ref_name) = git.parse_ref_path(path).unwrap();
        assert_eq!(ref_type, vcs::RefType::Tag);
        assert_eq!(ref_name, "0.1.1");
    }
}