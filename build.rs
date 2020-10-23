use std::process::Command;

fn main() {
    let git_hash_output = Command::new("git").args(&["rev-parse", "HEAD"]).output().unwrap();
    let git_hash = String::from_utf8(git_hash_output.stdout).unwrap();
    println!("cargo:rustc-env=GIT_HASH={}", git_hash);

    let toolset_hash_output = Command::new("bash").args(&["-c", r#"
        cat rsc/install/*.sh | sha1sum | cut -f 1 -d ' '
    "#]).output().unwrap();
    let toolset_hash = String::from_utf8(toolset_hash_output.stdout).unwrap();
    println!("cargo:rustc-env=TOOLSET_HASH={}", toolset_hash);
}
