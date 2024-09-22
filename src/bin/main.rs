use anyhow::{anyhow, Result};
use clap::{CommandFactory, Parser};
use shlf::{dotfile::Dotfiles, suggestions::suggest_dotfiles, Actions, Shelf};
use std::path::{Path, PathBuf};

use clap_complete::{generate, Generator};
use std::io;

fn print_completions<G: Generator>(gen: G, cmd: &mut clap::Command) {
    generate(gen, cmd, cmd.get_name().to_string(), &mut io::stdout());
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Shelf::parse();
    let config_dir = get_config_directory()?;
    let index_path = config_dir.join("index.json");
    let mut index = load_or_create_index(&config_dir, &index_path).await?;
    match cli.command {
        Actions::Add { paths } => {
            index.add_multi(paths).await;
        }
        Actions::Ls => index.print_list(),
        Actions::Rm { path } => {
            let results = index.remove_multi(&[path.to_str().unwrap()]);
            if results.iter().any(|r| r.is_err()) {
                return Err(anyhow!("Failed to remove one or more dotfiles"));
            }
        }
        Actions::Cp => index.copy().await?,
        Actions::Suggest { interactive } => suggest_dotfiles(&mut index, interactive).await?,
        Actions::Completion { shell } => {
            let mut cmd = Shelf::command();
            print_completions(shell, &mut cmd);
            return Ok(());
        }
    }

    index.save(&index_path).await?;
    Ok(())
}

fn get_config_directory() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow!("Failed to get config directory"))?
        .join("shelf");
    std::fs::create_dir_all(&config_dir)?;
    Ok(config_dir)
}

async fn load_or_create_index(config_dir: &Path, index_path: &Path) -> Result<Dotfiles> {
    if index_path.exists() {
        match tokio::fs::read_to_string(index_path).await {
            Ok(contents) => match serde_json::from_str(&contents) {
                Ok(index) => Ok(index),
                Err(_) => Dotfiles::new(config_dir.join("dotfiles")).await,
            },
            Err(_) => Dotfiles::new(config_dir.join("dotfiles")).await,
        }
    } else {
        Dotfiles::new(config_dir.join("dotfiles")).await
    }
}
