mod commit;
// mod github;
mod review;
mod shell;
mod utils;

pub mod app;
pub mod dotfs;

use crate::app::{Shelf, run_app};
use crate::dotfs::DotFs;
use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use std::env;
use std::process;
use tracing::level_filters::LevelFilter;

async fn initialize_tracing() {
    let level = match env::var("RUST_LOG")
        .unwrap_or_else(|_| "off".to_string())
        .to_lowercase()
        .as_str()
    {
        "trace" => LevelFilter::TRACE,
        "debug" => LevelFilter::DEBUG,
        "info" => LevelFilter::INFO,
        "warn" => LevelFilter::WARN,
        "error" => LevelFilter::ERROR,
        _ => LevelFilter::OFF,
    };

    if let Err(e) = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false)
        .try_init()
    {
        eprintln!("Failed to initialize tracing: {}", e.to_string().red());
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    initialize_tracing().await;
    colored::control::set_override(true);

    let cli = Shelf::parse();
    let dotfs = DotFs::default();

    if let Err(err) = run_app(cli, dotfs).await {
        eprintln!("Error: {}", err.to_string().red());
        process::exit(1);
    }
    Ok(())
}
