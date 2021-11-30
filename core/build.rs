use std::process::Command;

fn main() {
    let git_hash_output = Command::new("git").args(&["rev-parse", "HEAD"]).output().unwrap();
    let git_hash = String::from_utf8(git_hash_output.stdout).unwrap();
    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
}
