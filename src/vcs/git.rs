use std::error::Error;
use std::collections::HashMap;

use maplit::hashmap;

use crate::config;
use crate::shell;
use crate::util::defer;

pub struct Git<'a, S: shell::Shell<'a> = shell::Default<'a>> {
    config: &'a config::Config<'a>,
    username: String,
    email: String,
    shell: S,
}

pub trait GitFeatures<'a> {
    fn new(username: &str, email: &str, config: &'a config::Config<'a>) -> Self;
    fn current_branch(&self) -> Result<String, Box<dyn Error>>;
    fn commit_hash(&self) -> Result<String, Box<dyn Error>>;
    fn remote_origin(&self) -> Result<String, Box<dyn Error>>;
    fn rebase_with_remote_counterpart(
        &self, url: &str, remote_branch: &str
    ) -> Result<String, Box<dyn Error>>;
    fn push(
        &self, url: &str, remote_branch: &str, msg: &str, 
        patterns: &Vec<&str>, options: &HashMap<&str, &str>
    ) -> Result<bool, Box<dyn Error>>;
}

pub trait GitHubFeatures<'a> {
    fn pr(
        &self, title: &str, head_branch: &str, base_branch: &str, options: &HashMap<&str, &str>
    ) -> Result<(), Box<dyn Error>>;    
}

impl<'a, S: shell::Shell<'a>> GitFeatures<'a> for Git<'a, S> {
    fn new(username: &str, email: &str, config: &'a config::Config<'a>) -> Git<'a, S> {
        return Git::<'a, S> {
            config,
            username: username.to_string(),
            email: email.to_string(),
            shell: S::new(config)
        }
    }
    fn current_branch(&self) -> Result<String, Box<dyn Error>> {
        Ok(self.shell.output_of(&vec!(
            "git", "symbolic-ref" , "--short", "HEAD"
        ), &hashmap!{})?)
    }
    fn commit_hash(&self) -> Result<String, Box<dyn Error>> {
        Ok(self.shell.output_of(&vec!(
            "git", "rev-parse" , "--short", "HEAD"
        ), &hashmap!{})?)
    }
    fn remote_origin(&self) -> Result<String, Box<dyn Error>> {
        Ok(self.shell.output_of(&vec!(
            "git", "config", "--get", "remote.origin.url"
        ), &hashmap!{})?)
    }
    fn push(
        &self, url: &str, remote_branch: &str, msg: &str, 
        patterns: &Vec<&str>, options: &HashMap<&str, &str> 
    ) -> Result<bool, Box<dyn Error>> {
        let use_lfs = match options.get("use-lfs") {
            Some(v) => *v == "yes",
            None => false
        };
        if use_lfs {
            // this useless diffing is for making lfs tracked files refreshed.
            // otherwise if lfs tracked file is written, codes below seems to treat these write as git diff.
            // even if actually no change.        
		    self.shell.eval("git --no-pager diff > /dev/null", &hashmap!{}, false)?;
        }
		let mut changed = false;

		for pattern in patterns {
            self.shell.exec(&vec!("git", "add", "-N", pattern), &hashmap!{}, false)?;
            let diff = self.shell.exec(&vec!("git", "add", "-n", pattern), &hashmap!{}, true)?;
			if !diff.is_empty() {
                log::info!("diff found for {} [{}]", pattern, diff);
                self.shell.exec(&vec!("git", "add", pattern), &hashmap!{}, false)?;
				changed=true
            }
        }
		if changed {
			log::info!("skip push because no changes for provided pattern [{}]", patterns.join(" "));
			return Ok(false)
        } else {
			if use_lfs {
				self.shell.eval("git lfs fetch --all > /tmp/lfs_error 2>&1", &hashmap!{}, false)?;
            }
			self.shell.exec(&vec!("git", "commit", "-m", msg), &hashmap!{}, false)?;
			log::info!("commit done: [{}]", msg);
			match self.config.release_target() {
                Some(_) => {
                    let b = self.current_branch()?;
                    // update remote counter part of the deploy branch again
                    // because sometimes other commits are made (eg. merging pull request, job updates metadata)
                    // here, $CI_BASE_BRANCH_NAME before colon means branch which name is $CI_BASE_BRANCH_NAME at remote `latest`
                    self.shell.exec(&vec!(
                        "git", "fetch", "--force", "latest", &format!("{}:remotes/latest/{}", b, b)
                    ), &hashmap!{}, false)?;
                    // deploy branch: rebase CI branch with remotes `latest`. 
                    // because if other changes commit to the branch, below causes push error without rebasing it
                    self.shell.exec(&vec!(
                        "git", "rebase", &format!("remotes/latest/{}", b)
                    ), &hashmap!{}, false)?;
                },
                None => {}
            }
			if use_lfs {
                self.shell.exec(&vec!("git", "lfs", "push", url, "--all"), &hashmap!{}, false)?;
            }
            self.shell.exec(&vec!(
                "git", "push", "--no-verify", url, &format!("HEAD:{}", remote_branch)
            ), &hashmap!{}, false)?;
			return Ok(true)
        }
    }
    fn rebase_with_remote_counterpart(
        &self, url: &str, remote_branch: &str
    ) -> Result<String, Box<dyn Error>> {
        self.shell.exec(&vec!("git", "config", "--global", "user.email", &self.email), &hashmap!{}, false)?;
        self.shell.exec(&vec!("git", "config", "--global", "user.name", &self.username), &hashmap!{}, false)?;
        self.shell.exec(&vec!("git", "remote", "add", "latest", url), &hashmap!{}, false)?;
        // defered removal of latest 
        defer!(
            self.shell.exec(&vec!(
                "git", "remote", "remove", "latest"
            ), &hashmap!{}, false).unwrap();
        );
        // we cannot `git pull latest $remote_branch` here. eg. $remote_branch = master case on circleCI. 
        // sometimes latest/master and master diverged, and pull causes merge FETCH_HEAD into master.
        // then it raises error if no mail/user name specified, because these are diverges branches. 
        // (but we don't know how master and latest/master are diverged with cached .git of circleCI)
        let base = self.shell.exec(&vec!(
            "git", "rev-parse", 
            &format!("{}^", &self.commit_hash()?
        )), &hashmap!{}, true)?;

        // here, $CI_BASE_BRANCH_NAME before colon means branch which name i $CI_BASE_BRANCH_NAME at remote `latest`
        self.shell.exec(&vec!(
            "git", "fetch", "--force", "latest", 
            &format!("{}:remotes/latest/{}", remote_branch, remote_branch)
        ), &hashmap!{}, false)?;
        /* if run_on_pr_branch {
            // pull request: forcefully match base branch and its remote `latest` counterpart
            // here, $CI_BASE_BRANCH_NAME menas local branch which name is $$CI_BASE_BRANCH_NAME
            self.shell.exec(&vec!(
                "git", "branch", "-f", remote_branch,
                &format!("remotes/latest/{}", remote_branch)
            ), &hashmap!{}, false)?;
        } else */ {
            // deploy branch: rebase CI branch with remotes `latest`. 
            // because sometimes build on deploy branch made commit to $CI_BRANCH (eg. commit meta data)
            self.shell.exec(&vec!(
                "git", "rebase", &format!("remotes/latest/{}", remote_branch)
            ), &hashmap!{}, false)?;
        }
        Ok(self.shell.output_of(
            &vec!("git", "diff", "--name-only", &format!("{}...HEAD", base)),
            &hashmap!{}
        )?)
    }
}

impl<'a, S: shell::Shell<'a>> GitHubFeatures<'a> for Git<'a, S> {
    fn pr(
        &self, title: &str, head_branch: &str, base_branch: &str, options: &HashMap<&str, &str>
    ) -> Result<(), Box<dyn Error>> {
        match options.get("labels") {
            Some(l) => {
                self.shell.exec(&vec!(
                    "hub", "pull-request", "-f", "-m", title, 
                    "-h", head_branch, "-b", base_branch, "-l", l
                ), &hashmap!{}, false)?;
            },
            None => {
                self.shell.exec(&vec!(
                    "hub", "pull-request", "-f", "-m", title, 
                    "-h", head_branch, "-b", base_branch,
                ), &hashmap!{}, false)?;
            }
        }
        Ok(())
    }    
}