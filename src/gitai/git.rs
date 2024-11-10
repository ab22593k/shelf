use std::process::Command;

use anyhow::Result;
use git2::DiffOptions;

pub fn git_diff() -> Result<String> {
    let output = Command::new("git")
        .arg("diff-index")
        .arg("HEAD")
        .arg("--stat")
        .arg("-p")
        .output()
        .expect("failed to execute process");

    let stdout = String::from_utf8(output.stdout)?;

    Ok(stdout)
}
