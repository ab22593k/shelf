#![allow(clippy::nonminimal_bool)]

use anyhow::{anyhow, Result};
use clap::{CommandFactory, Parser};
use shlf::{dotfile, suggestions::Suggestions, Actions, Shelf};

pub use dotfile::Dotfiles;

use clap_complete::{generate, Generator};
use std::{io, path::PathBuf};

fn print_completions<G: Generator>(gen: G, cmd: &mut clap::Command) {
    generate(gen, cmd, cmd.get_name().to_string(), &mut io::stdout());
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Shelf::parse();
    let config_dir = directories::BaseDirs::new()
        .map(|base_dirs| base_dirs.config_dir().join("shelf"))
        .or_else(|| {
            std::env::var("XDG_CONFIG_HOME")
                .ok()
                .map(|x| PathBuf::from(x).join("shelf"))
        })
        .or_else(|| home::home_dir().map(|x| x.join(".config").join("shelf")))
        .unwrap_or_else(|| {
            eprintln!("Warning: Could not determine config directory. Using current directory.");
            std::env::current_dir().unwrap().join(".shelf")
        });
    let target_directory = config_dir.join("dotfiles");
    let index_file_path = config_dir.join("index.json");
    tokio::fs::create_dir_all(&target_directory).await?;
    let mut df = Dotfiles::load(config_dir.clone(), target_directory).await?;

    match cli.command {
        Actions::Add { paths } => {
            df.add_multi(paths).await;
        }
        Actions::List => df.print_list(),
        Actions::Remove { path } => {
            let results = df.remove_multi(&[path.to_str().unwrap()]);
            if results.iter().any(|r| r.is_err()) {
                return Err(anyhow!("Failed to remove one or more dotfiles"));
            }
        }
        Actions::Copy => {
            df.copy().await?;
        }
        Actions::Repo { path, push, pull } => {
            let repo_path = path;

            if push {
                let output = std::process::Command::new("gh")
                    .arg("repo")
                    .arg("create")
                    .arg("--source")
                    .arg(&repo_path)
                    .arg("--push")
                    .output()?;

                if !output.status.success() {
                    return Err(anyhow!(
                        "Failed to create and push repository: {}",
                        String::from_utf8_lossy(&output.stderr)
                    ));
                }
                println!("Repository created and pushed successfully.");
            }

            if pull {
                let output = std::process::Command::new("gh")
                    .arg("repo")
                    .arg("clone")
                    .arg(&repo_path)
                    .output()?;

                if !output.status.success() {
                    return Err(anyhow!(
                        "Failed to clone repository: {}",
                        String::from_utf8_lossy(&output.stderr)
                    ));
                }
                println!("Repository cloned successfully.");
            }
        }
        Actions::Suggest { interactive } => {
            Suggestions::default().render(&mut df, interactive).await?
        }

        Actions::Completion { shell } => {
            let mut cmd = Shelf::command();
            print_completions(shell, &mut cmd);
            return Ok(());
        }
    }

    df.save(&index_file_path).await?;
    Ok(())
}
