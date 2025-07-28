pub mod app;
pub mod commit;
pub mod config;
pub mod dots;
pub mod error;
pub mod review;
pub mod shell;
pub mod utils;

use crate::app::{Shelf, run_app};
use crate::config::init_dots_repo;
use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use std::process;

#[cfg(debug_assertions)]
use std::env;

#[cfg(debug_assertions)]
use tracing::level_filters::LevelFilter;

#[cfg(debug_assertions)]
async fn initialize_tracing() {
    let trace_granularity = match env::var("RUST_LOG")
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

    if let Err(tracing_blip) = tracing_subscriber::fmt()
        .with_max_level(trace_granularity)
        .with_target(false)
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .pretty()
        .try_init()
    {
        eprintln!(
            "Failed to initialize tracing: {}",
            tracing_blip.to_string().red()
        );
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(debug_assertions)]
    initialize_tracing().await;

    colored::control::set_override(true);

    let user_directive = Shelf::parse();
    let config_nexus = init_dots_repo()?;

    if let Err(operation_fizzle) = run_app(user_directive, config_nexus).await {
        eprintln!("Error: {}", operation_fizzle.to_string().red());
        process::exit(1);
    }
    Ok(())
}
