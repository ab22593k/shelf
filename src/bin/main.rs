use anyhow::Result;
// use clap::Parser;
use shelf::{Commands, SlfActions, SlfCLI, SlfIndex};
// use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    use clap::Parser;

    let cli = SlfCLI::parse();
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Failed to get config directory"))?
        .join("shelf");
    std::fs::create_dir_all(&config_dir)?;

    let index_path = config_dir.join("index.json");
    let mut index = if index_path.exists() {
        match tokio::fs::read_to_string(&index_path).await {
            Ok(contents) => match serde_json::from_str(&contents) {
                Ok(index) => index,
                Err(_) => SlfIndex::new(config_dir.join("dotfiles")).await?,
            },
            Err(_) => SlfIndex::new(config_dir.join("dotfiles")).await?,
        }
    } else {
        SlfIndex::new(config_dir.join("dotfiles")).await?
    };

    match &cli.command {
        Some(Commands::Dotfiles { action }) => match action {
            SlfActions::Track { path } => index.add_ref(path).await?,
            SlfActions::List => {
                for (name, dotfile) in index.list() {
                    println!(
                        "{}: {} -> {}",
                        name,
                        dotfile.source().display(),
                        dotfile.target().display()
                    );
                }
            }
            SlfActions::Remove { path } => index.remove_ref(path.to_str().unwrap())?,
            SlfActions::Sync => index.do_sync().await?,
        },
        None => println!("No subcommand was used"),
    }

    tokio::fs::write(&index_path, serde_json::to_string(&index)?).await?;
    Ok(())
}
