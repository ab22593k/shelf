use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    dotconf: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        let config_d = directories::BaseDirs::new()
            .map(|base_dirs| base_dirs.config_dir().join("shelf"))
            .or_else(|| {
                std::env::var("XDG_CONFIG_HOME")
                    .ok()
                    .map(|x| PathBuf::from(x).join("shelf"))
            })
            .or_else(|| home::home_dir().map(|x| x.join(".config").join("shelf")))
            .unwrap_or_else(|| std::env::current_dir().unwrap().join(".shelf"));

        Self {
            dotconf: config_d.join("dotconf"),
        }
    }
}
