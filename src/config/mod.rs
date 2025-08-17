use anyhow::Result;
use std::path::PathBuf;

use crate::{app::dots::Dots, error::Shelfor};

const HIDDEN_VAULT_DIR: &str = ".shelf";

pub fn init_bare_repo() -> Result<Dots> {
    let config_home_base = std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| Shelfor::HomeDirectoryNotFound)?
        .canonicalize()?;
    let vault_core_path = config_home_base.join(HIDDEN_VAULT_DIR);

    Dots::new(vault_core_path, config_home_base)
}
