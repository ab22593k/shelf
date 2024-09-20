use anyhow::Result;
use clap::Parser;
use shelf::{
    suggestions::{interactive_selection, print_suggestions},
    SlfActions, SlfCLI, SlfIndex,
};
use std::path::{Path, PathBuf};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = SlfCLI::parse();
    let config_dir = get_config_directory()?;
    let index_path = config_dir.join("index.json");
    let mut index = load_or_create_index(&config_dir, &index_path).await?;

    match cli.command {
        SlfActions::Track { path } => track_dotfile(&mut index, &path).await?,
        SlfActions::List => list_dotfiles(&index),
        SlfActions::Remove { path } => remove_dotfile(
            &mut index,
            path.as_path().to_str().expect("conversion failed"),
        )?,
        SlfActions::Sync => index.do_sync().await?,
        SlfActions::Suggest { interactive } => suggest_dotfiles(&mut index, interactive).await?,
    }

    save_index(&index_path, &index).await?;
    Ok(())
}

fn get_config_directory() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Failed to get config directory"))?
        .join("shelf");
    std::fs::create_dir_all(&config_dir)?;
    Ok(config_dir)
}

async fn load_or_create_index(config_dir: &Path, index_path: &Path) -> Result<SlfIndex> {
    if index_path.exists() {
        match tokio::fs::read_to_string(index_path).await {
            Ok(contents) => match serde_json::from_str(&contents) {
                Ok(index) => Ok(index),
                Err(_) => SlfIndex::new(config_dir.join("dotfiles")).await,
            },
            Err(_) => SlfIndex::new(config_dir.join("dotfiles")).await,
        }
    } else {
        SlfIndex::new(config_dir.join("dotfiles")).await
    }
}

async fn track_dotfile(index: &mut SlfIndex, path: &Path) -> Result<()> {
    index.add_ref(path).await
}

fn list_dotfiles(index: &SlfIndex) {
    for (name, dotfile) in index.list() {
        println!(
            "{}: {} -> {}",
            name,
            dotfile.source().display(),
            dotfile.target().display()
        );
    }
}

fn remove_dotfile(index: &mut SlfIndex, path: &str) -> Result<()> {
    index.remove_ref(path)
}

async fn suggest_dotfiles(index: &mut SlfIndex, interactive: bool) -> Result<()> {
    if interactive {
        match interactive_selection() {
            Ok(selected_files) => {
                for file in selected_files {
                    let expanded_path = shellexpand::tilde(&file);
                    match index.add_ref(expanded_path.as_ref()).await {
                        Ok(_) => println!("Added: {}", file),
                        Err(e) => println!("Failed to add {}: {}", file, e),
                    }
                }
            }
            Err(e) => println!("Error during interactive selection: {}", e),
        }
    } else {
        print_suggestions();
    }
    Ok(())
}

async fn save_index(index_path: &PathBuf, index: &SlfIndex) -> Result<()> {
    tokio::fs::write(index_path, serde_json::to_string(index)?).await?;
    Ok(())
}
