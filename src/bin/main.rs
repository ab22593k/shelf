use anyhow::{anyhow, Result};
use clap::{CommandFactory, Parser};
use shlf::{
    dotfile::Dotfiles,
    suggestions::{interactive_selection, print_suggestions},
    Actions, Slf,
};
use std::path::{Path, PathBuf};

use clap_complete::{generate, Generator};
use std::io;

fn print_completions<G: Generator>(gen: G, cmd: &mut clap::Command) {
    generate(gen, cmd, cmd.get_name().to_string(), &mut io::stdout());
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Slf::parse();
    let config_dir = get_config_directory()?;
    let index_path = config_dir.join("index.json");
    let mut index = load_or_create_index(&config_dir, &index_path).await?;

    match cli.command {
        Actions::Track { paths } => {
            for path in paths {
                track_dotfile(&mut index, &path).await?;
            }
        }
        Actions::List => list_dotfiles(&index),
        Actions::Remove { path } => remove_dotfile(
            &mut index,
            path.as_path().to_str().expect("conversion failed"),
        )?,
        Actions::Sync => index.sync_dotfiles().await?,
        Actions::Suggest { interactive } => suggest_dotfiles(&mut index, interactive).await?,
        Actions::Completion { shell } => {
            let mut cmd = Slf::command();
            print_completions(shell, &mut cmd);
            return Ok(());
        }
    }

    save_index(&index_path, &index).await?;
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
async fn track_dotfile(index: &mut Dotfiles, path: &Path) -> Result<()> {
    match index.add_dotfile(path).await {
        Ok(_) => println!("Added: {}", path.display()),
        Err(e) => eprintln!("Failed to add {}: {}", path.display(), e),
    }
    Ok(())
}
fn list_dotfiles(index: &Dotfiles) {
    for (name, dotfile_entry) in index.list_dotfiles() {
        println!(
            "{}: {} -> {}",
            name,
            dotfile_entry.source.display(),
            index.target_directory.join(name).display()
        );
    }
}
fn remove_dotfile(index: &mut Dotfiles, path: &str) -> Result<()> {
    index
        .dotfiles
        .remove(path)
        .ok_or_else(|| anyhow!("Dotfile not found: {}", path))?;
    Ok(())
}
async fn suggest_dotfiles(index: &mut Dotfiles, interactive: bool) -> Result<()> {
    if interactive {
        match interactive_selection() {
            Ok(selected_files) => {
                for file in selected_files {
                    let expanded_path = shellexpand::tilde(&file);
                    match index.add_dotfile(expanded_path.as_ref()).await {
                        Ok(_) => println!("Added: {}", file),
                        Err(e) => eprintln!("Failed to add {}: {}", file, e),
                    }
                }
            }
            Err(e) => eprintln!("Error during interactive selection: {}", e),
        }
    } else {
        print_suggestions();
    }
    Ok(())
}
async fn save_index(index_path: &PathBuf, index: &Dotfiles) -> Result<()> {
    tokio::fs::write(index_path, serde_json::to_string(index)?).await?;
    Ok(())
}
