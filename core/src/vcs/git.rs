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

use base64;
use maplit::hashmap;

use crate::shell;
use crate::util::{defer, escalate};
use crate::vcs;



// because defer uses Drop trait behaviour, this cannot be de-duped as function
macro_rules! setup_remote {
    ($git:expr, $url:expr) => {
        $git.shell.exec(&vec!(
            "git", "remote", "add", "latest", $url
        ), shell::no_env(), shell::no_cwd(), &shell::no_capture())?;
        // defered removal of latest
        defer! {
            $git.shell.exec(&vec!(
                "git", "remote", "remove", "latest"
            ), shell::no_env(), shell::no_cwd(), &shell::no_capture()).unwrap();
        };
    };
}

pub struct Git<S: shell::Shell = shell::Default> {
    credential: RemoteCredential,
    shell: S,
    #[cfg(feature="git2")]
    repo: Repository,
    #[cfg(not(feature="git2"))]
    repo: StubRepository,
}

pub struct RemoteCredential {
    pub username: String,
    pub email: String,
    pub key: String,
}
impl RemoteCredential {
    pub fn authorize<'a>(&'a self, command: Vec<&'a str>, target_url: &str) -> Result<Vec<String>, Box<dyn Error>> {
        let mut authorized = vec![];
        if command[0] != "git" {
            return escalate!(Box::new(vcs::VCSError {
                cause: format!("RemoteCredential::authorize only authorizes git command: {:?}", command)
            }));
        }
        if !target_url.starts_with("https://") {
            return escalate!(Box::new(vcs::VCSError {
                cause: format!("RemoteCredential::authorize only authorizes access to https url. ssh support is not yet. {}", target_url)
            }));
        }
        authorized.push(vec![command[0].to_string()]);
        // add option like
        // -c "http.https://github.com/.extraheader=" 
        // -c "http.https://github.com/<owner>/<repo>/.extraheader=AUTHORIZATION: Basic <base64 encoded user and pass>"
        let re = regex::Regex::new(r"(^.+[:/])[^/\.]+/[^/\.]+").unwrap();
        let base_url = match re.captures(&target_url) {
            Some(c) => match c.get(1) {
                Some(m) => m.as_str().to_string(),
                None => return escalate!(Box::new(vcs::VCSError {
                    cause: format!("invalid remote origin url: {}", target_url)
                }))
            },
            None => return escalate!(Box::new(vcs::VCSError {
                cause: format!("invalid remote origin url: {}", target_url)
            }))
        };
        // by regex, base_url contains last /
        authorized.push(vec!["-c".to_string(), format!("http.{}.extraheader=", base_url)]);
        authorized.push(vec!["-c".to_string(), format!(
                "http.{}/.extraheader=AUTHORIZATION: Basic {}", 
                if target_url.ends_with("/") { &target_url[0..target_url.len()-1] } else { target_url },
                base64::encode(&format!("{}:{}" , self.username, self.key))
            )
        ]);
        authorized.push(
            command.iter().enumerate()
                .filter(|(idx, _)| *idx > 0)
                .map(|(_, v)| v.to_string())
                .collect::<Vec<String>>()
        );
        Ok(authorized.concat())
    }
}

pub trait GitFeatures<S: shell::Shell> {
    fn new(username: &str, email: &str, key: &str, shell: S) -> Self;
    fn current_ref(&self) -> Result<(vcs::RefType, String), Box<dyn Error>>;
    fn delete_branch(&self, remote_url: &str, ref_type: vcs::RefType, ref_path: &str) -> Result<(), Box<dyn Error>>;
    fn fetch_branch(&self, remote_url: &str, branch_name: &str) -> Result<(), Box<dyn Error>>;
    fn squash_branch(&self, n: usize) -> Result<(), Box<dyn Error>>;
    fn commit_hash(&self, expr: Option<&str>) -> Result<String, Box<dyn Error>>;
    fn checkout(&self, commit: &str, branch_name: Option<&str>) -> Result<(), Box<dyn Error>>;
    fn remote_url(&self, remote_name: Option<&str>) -> Result<String, Box<dyn Error>>;
    fn repository_root(&self) -> Result<String, Box<dyn Error>>;
    fn diff_paths(&self, expression: &str) -> Result<String, Box<dyn Error>>;
    fn rebase_with_remote_counterpart(
        &self, remote_url: &str, remote_branch: &str
    ) -> Result<(), Box<dyn Error>>;
    fn cherry_pick(&self, branch: &str) -> Result<(), Box<dyn Error>>;
    fn push_branch(
        &self, remote_url: &str, local_ref: &str, 
        remote_branch: &str, option: &HashMap<&str, &str>
    ) -> Result<(), Box<dyn Error>>;
    fn push_diff(
        &self, remote_url: &str, remote_branch: &str, msg: &str, 
        patterns: &Vec<&str>, options: &HashMap<&str, &str>
    ) -> Result<bool, Box<dyn Error>>;
    fn tags(&self) -> Result<Vec<String>, Box<dyn Error>>;
}

impl<S: shell::Shell> Git<S> {
    fn commit_env(&self) -> HashMap<String, String> {
        hashmap!{
            "GIT_COMMITTER_NAME".to_string() => self.credential.username.to_string(),
            "GIT_AUTHOR_NAME".to_string() => self.credential.username.to_string(),
            "GIT_COMMITTER_EMAIL".to_string() => self.credential.email.to_string(),
            "GIT_AUTHOR_EMAIL".to_string() => self.credential.email.to_string(),
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
    // TODO: I'm sure that rust has smarter way to do this, rewrite with that way
    fn to_str_vec<'a>(vec: &'a Vec<String>) -> Vec<&'a str> {
        vec.iter().map(|v| v.as_str()).collect()
    }
}

impl<S: shell::Shell> GitFeatures<S> for Git<S> {
    fn new(username: &str, email: &str, key: &str, shell: S) -> Git<S> {
        #[cfg(feature="git2")]
        let cwd = std::env::current_dir().unwrap();
        return Git::<S> {
            credential: RemoteCredential {
                username: username.to_string(),
                email: email.to_string(),
                key: key.to_string(),
            },
            shell,
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
    fn delete_branch(&self, remote_url: &str, ref_type: vcs::RefType, ref_path: &str) -> Result<(), Box<dyn Error>> {
        match ref_type {
            vcs::RefType::Branch => {
                self.shell.exec(&vec![
                    "git", "branch", "-D", ref_path
                ], shell::no_env(), shell::no_cwd(), &shell::no_capture())?;
            },
            vcs::RefType::Remote => {
                self.shell.exec(&Self::to_str_vec(&self.credential.authorize(vec![
                    "git", "push", remote_url, "--delete", ref_path
                ], remote_url)?), shell::no_env(), shell::no_cwd(), &shell::no_capture())?;
            }
            _ => {
                return escalate!(Box::new(vcs::VCSError {
                    cause: format!("delete_branch: unsupported ref type: {}/{}", ref_type, ref_path)
                }))
            }
        }
        Ok(())
    }
    fn fetch_branch(&self, remote_url: &str, branch_name: &str) -> Result<(), Box<dyn Error>> {
        self.shell.exec(&Self::to_str_vec(&self.credential.authorize(vec![
            "git", "fetch", remote_url, branch_name
        ], remote_url)?), shell::no_env(), shell::no_cwd(), &shell::no_capture())?;
        Ok(())
    }
    fn squash_branch(&self, n: usize) -> Result<(), Box<dyn Error>> {
        self.shell.exec(&vec!(
            "git", "rebase", "-i", &format!("HEAD~{}", n)
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
    fn remote_url(&self, remote_name: Option<&str>) -> Result<String, Box<dyn Error>> {
        let remote = remote_name.unwrap_or("origin");
        if cfg!(feature="git2") {
            let origin = self.repo.find_remote(remote)?;
            Ok(origin.url().unwrap().to_string())
        } else {
            Ok(self.shell.output_of(&vec!(
                "git", "config", "--get", &format!("remote.{}.url", remote)
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
        ), self.commit_env(), shell::no_cwd(), &shell::capture())?;
        Ok(())        
    }
    fn push_branch(
        &self, remote_url: &str, local_ref: &str, 
        remote_branch: &str, options: &HashMap<&str, &str>
    ) -> Result<(), Box<dyn Error>> {
        let explicit_lfs = match options.get("explicit_lfs") {
            Some(v) => !v.is_empty(),
            None => false
        };
        if match options.get("new") {
            Some(v) => v.is_empty(),
            None => true
        } {
            // if the branch already exists, refresh remote branch with its latest state
            self.rebase_with_remote_counterpart(remote_url, remote_branch)?;
        }
        if explicit_lfs {
            self.shell.exec(
                &vec!["git", "lfs", "fetch"], shell::no_env(), shell::no_cwd(), &shell::no_capture()
            )?;
            self.shell.exec(
                &Self::to_str_vec(&self.credential.authorize(vec!["git", "lfs", "push", remote_url], remote_url)?),
                self.commit_env(), shell::no_cwd(), &shell::no_capture()
            )?;
        }
        self.shell.exec(&Self::to_str_vec(&self.credential.authorize(vec![
            "git", "push", "--no-verify", remote_url, &format!("{}:{}", local_ref, remote_branch)
        ], remote_url)?), self.commit_env(), shell::no_cwd(), &shell::no_capture())?;
        Ok(())
    }
    fn push_diff(
        &self, remote_url: &str, remote_branch: &str, msg: &str, 
        patterns: &Vec<&str>, options: &HashMap<&str, &str> 
    ) -> Result<bool, Box<dyn Error>> {
        let explicit_lfs = match options.get("explicit_lfs") {
            Some(v) => !v.is_empty(),
            None => false
        };
        let original_ref = self.commit_hash(None)?;
        defer! {
            self.shell.exec(
                &vec!["git", "reset", "--hard", &original_ref],
                shell::no_env(), shell::no_cwd(), &shell::capture()
            ).unwrap();
        };
        if explicit_lfs {
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
			if explicit_lfs {
				self.shell.exec(
                    &vec!["git", "lfs", "fetch"], shell::no_env(), shell::no_cwd(), &shell::no_capture()
                )?;
            }
			self.shell.exec(&vec!("git", "commit", "-m", msg), self.commit_env(), shell::no_cwd(), &shell::no_capture())?;
			log::debug!("commit done: [{}]", msg);
			if explicit_lfs {
                self.shell.exec(
                    &Self::to_str_vec(&self.credential.authorize(vec!["git", "lfs", "push", remote_url], remote_url)?), 
                    self.commit_env(), shell::no_cwd(), &shell::no_capture()
                )?;
            }
            self.shell.exec(&Self::to_str_vec(&self.credential.authorize(vec![
                "git", "push", "--no-verify", remote_url, &format!("HEAD:{}", remote_branch)
            ], remote_url)?),  self.commit_env(), shell::no_cwd(), &shell::no_capture())?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;

    #[test]
    fn parse_ref_path_test() {
        let config = config::Config::dummy(None).unwrap();
        let git = Git::<shell::Default>::new(
            "umegaya", "mail@address.com", "key", shell::new_default(&config)
        );
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