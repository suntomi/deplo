use std::error::Error;
use std::collections::HashMap;

use maplit::hashmap;

use crate::config;
use crate::shell;
use crate::util::defer;

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
}

pub trait GitFeatures {
    fn new(username: &str, email: &str, config: &config::Container) -> Self;
    fn current_branch(&self) -> Result<String, Box<dyn Error>>;
    fn commit_hash(&self) -> Result<String, Box<dyn Error>>;
    fn remote_origin(&self) -> Result<String, Box<dyn Error>>;
    fn repository_root(&self) -> Result<String, Box<dyn Error>>;
    fn rebase_with_remote_counterpart(
        &self, url: &str, remote_branch: &str
    ) -> Result<String, Box<dyn Error>>;
    fn push(
        &self, url: &str, remote_branch: &str, msg: &str, 
        patterns: &Vec<&str>, options: &HashMap<&str, &str>
    ) -> Result<bool, Box<dyn Error>>;
}

pub trait GitHubFeatures {
    fn pr(
        &self, title: &str, head_branch: &str, base_branch: &str, options: &HashMap<&str, &str>
    ) -> Result<(), Box<dyn Error>>;    
}

impl<S: shell::Shell> Git<S> {
    fn setup_author(&self) -> Result<(), Box<dyn Error>> {
        log::info!("git: setup {}/{}", self.email, self.username);
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
        return Git::<S> {
            config: config.clone(),
            username: username.to_string(),
            email: email.to_string(),
            shell: S::new(config)
        }
    }
    fn current_branch(&self) -> Result<String, Box<dyn Error>> {
        Ok(self.shell.output_of(&vec!(
            "git", "symbolic-ref" , "--short", "HEAD"
        ), shell::no_env(), shell::no_cwd())?)
    }
    fn commit_hash(&self) -> Result<String, Box<dyn Error>> {
        Ok(self.shell.output_of(&vec!(
            "git", "rev-parse" , "HEAD"
        ), shell::no_env(), shell::no_cwd())?)
    }
    fn remote_origin(&self) -> Result<String, Box<dyn Error>> {
        Ok(self.shell.output_of(&vec!(
            "git", "config", "--get", "remote.origin.url"
        ), shell::no_env(), shell::no_cwd())?)
    }
    fn repository_root(&self) -> Result<String, Box<dyn Error>> {
        Ok(self.shell.output_of(&vec!(
            "git", "rev-parse", "--show-toplevel"
        ), shell::no_env(), shell::no_cwd())?)
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
		    self.shell.eval("git --no-pager diff > /dev/null", shell::default(), shell::no_env(), shell::no_cwd(), false)?;
        }
		let mut changed = false;

		for pattern in patterns {
            self.shell.exec(&vec!("git", "add", "-N", pattern), shell::no_env(), shell::no_cwd(), false)?;
            let diff = self.shell.exec(&vec!("git", "add", "-n", pattern), shell::no_env(), shell::no_cwd(), true)?;
			if !diff.is_empty() {
                log::info!("diff found for {} [{}]", pattern, diff);
                self.shell.exec(&vec!("git", "add", pattern), shell::no_env(), shell::no_cwd(), false)?;
				changed = true
            }
        }
		if !changed {
			log::info!("skip push because no changes for provided pattern [{}]", patterns.join(" "));
			return Ok(false)
        } else {
			if use_lfs {
                // TODO_PATH: use Path to generate path of /tmp/lfs_error
				self.shell.eval(
                    "git lfs fetch --all > /tmp/lfs_error 2>&1", 
                    shell::default(), shell::no_env(), shell::no_cwd(), false
                )?;
            }
			self.shell.exec(&vec!("git", "commit", "-m", msg), shell::no_env(), shell::no_cwd(), false)?;
			log::info!("commit done: [{}]", msg);
			match config.release_target() {
                Some(_) => {
                    setup_remote!(self, url);
                    let b = self.current_branch()?;
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
    ) -> Result<String, Box<dyn Error>> {
        let config = self.config.borrow();
        // user and email
        self.setup_author()?;
        if config.has_debug_option("skip_rebase") {
            return Ok(self.shell.output_of(
                &vec!("git", "diff", "--name-only", "HEAD~1...HEAD"),
                shell::no_env(), shell::no_cwd()
            )?)
        }
        setup_remote!(self, url);
        // get current base commit hash (that is, HEAD^1). 
        // because below we do rebase, which may change HEAD.
        // but we want to know the diff between "current" HEAD^1 and "rebased" HEAD with ... 
        // to invoke all possible deployment that need to run
        let commit = self.shell.output_of(&vec!(
            "git", "rev-parse" ,"HEAD"
        ), shell::no_env(), shell::no_cwd())?;
        let base = self.shell.exec(&vec!(
            "git", "rev-parse", &format!("{}^", commit)
        ), shell::no_env(), shell::no_cwd(), true)?;

        // we cannot `git pull latest $remote_branch` here. eg. $remote_branch = master case on circleCI. 
        // sometimes latest/master and master diverged, and pull causes merge FETCH_HEAD into master.
        // then it raises error if no mail/user name specified, because these are diverged branches. 
        // (but we don't know how master and latest/master are diverged with cached .git of circleCI)

        // here, remote_branch before colon means branch which name is $remote_branch at remote `latest`
        // fetch remote counter part of the branch, which may change HEAD by commiting something 
        self.shell.exec(&vec!(
            "git", "fetch", "--force", "latest", 
            &format!("{}:remotes/latest/{}", remote_branch, remote_branch)
        ), shell::no_env(), shell::no_cwd(), false)?;
        // because sometimes build on deploy branch made commit to $CI_BRANCH (eg. commit meta data)
        self.shell.exec(&vec!(
            "git", "rebase", &format!("remotes/latest/{}", remote_branch)
        ), shell::no_env(), shell::no_cwd(), false)?;
        // actually get diff
        Ok(self.shell.output_of(
            &vec!("git", "diff", "--name-only", &format!("{}...HEAD", base)),
            shell::no_env(), shell::no_cwd()
        )?)
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
}