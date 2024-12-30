use crate::{
    ai::utils::{install_git_hook, remove_git_hook},
    config::Config,
    ProviderAction,
};

use anyhow::Result;
use colored::Colorize;

pub async fn handle_provider_config(config: Config, action: ProviderAction) -> Result<()> {
    let mut config = config.read_all()?;
    match action {
        ProviderAction::Set { key, value } => {
            config.set(&key, &value)?;
            config.write_all().await?;
            println!("{} {} = {}", "Set:".green().bold(), key, value);
        }

        ProviderAction::Get { key } => {
            if let Some(value) = config.get(&key) {
                println!("{}", value);
            } else {
                println!("{} Key not found: {}", "Error:".red().bold(), key);
            }
        }

        ProviderAction::List => {
            println!("{}", "Configuration:".green().bold());
            config
                .list()
                .iter()
                .for_each(|(k, v)| println!("{} = {}", k, v));
        }

        ProviderAction::Hook { install, uninstall } => match git2::Repository::open_from_env() {
            Ok(repo) => {
                let hooks_dir = repo.path().join("hooks");

                if install {
                    match install_git_hook(&hooks_dir) {
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("{} Failed to install hook: {}", "Error:".red().bold(), e);
                        }
                    }
                } else if uninstall {
                    match remove_git_hook(&hooks_dir) {
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("{} Failed to uninstall hook: {}", "Error:".red().bold(), e);
                        }
                    }
                }
            }
            Err(e) => println!(
                "{} Failed to open git repository: {}",
                "Error:".red().bold(),
                e
            ),
        },
    }
    Ok(())
}
