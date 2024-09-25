#![allow(clippy::nonminimal_bool)]

use anyhow::{anyhow, Result};
use clap::{CommandFactory, Parser};
use shlf::{dotfile, suggestions::Suggestions, Actions, Shelf};

pub use dotfile::Dotfiles;

use clap_complete::{generate, Generator};
use std::io;

fn print_completions<G: Generator>(gen: G, cmd: &mut clap::Command) {
    generate(gen, cmd, cmd.get_name().to_string(), &mut io::stdout());
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Shelf::parse();
    let target_directory = std::path::PathBuf::from("~/.config/shelf");
    let mut index = Dotfiles::new(target_directory).await?;

    match cli.command {
        Actions::Add { paths } => {
            index.add_multi(paths).await;
        }
        Actions::List => index.print_list(),
        Actions::Remove { path } => {
            let results = index.remove_multi(&[path.to_str().unwrap()]);
            if results.iter().any(|r| r.is_err()) {
                return Err(anyhow!("Failed to remove one or more dotfiles"));
            }
        }
        Actions::Copy => index.copy().await?,
        Actions::Repo { path, push, pull } => {}
        Actions::Suggest { interactive } => {
            Suggestions::new().render(&mut index, interactive).await?
        }

        Actions::Completion { shell } => {
            let mut cmd = Shelf::command();
            print_completions(shell, &mut cmd);
            return Ok(());
        }
    }

    index.save(&index.target_directory).await?;
    Ok(())
}
