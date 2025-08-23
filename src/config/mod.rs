use anyhow::Result;
use directories::BaseDirs;
use serde::Deserialize;
use std::{fs, path::Path};

use crate::{app::dots::Dots, error::Shelfor};

const HIDDEN_VAULT_DIR: &str = ".shelf";

/// Configuration for the prompt generation, loaded from `shelf.toml`.
#[derive(Deserialize, Default, Debug, Clone)]
pub(super) struct PromptConfig {
    #[serde(default)]
    pub(crate) skip_directories: Vec<String>,
    #[serde(default)]
    pub(crate) skip_files: Vec<String>,
}

/// Main configuration structure, mirroring `shelf.toml`.
#[derive(Deserialize, Default, Debug, Clone)]
pub(super) struct Config {
    #[serde(default)]
    pub(crate) prompt: PromptConfig,
}

pub fn init_bare_repo() -> Result<Dots> {
    let home_dir = BaseDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .ok_or(Shelfor::HomeDirectoryNotFound)?;

    let canonical_home = home_dir.canonicalize()?;
    let vault_core_path = canonical_home.join(HIDDEN_VAULT_DIR);

    Dots::new(vault_core_path, canonical_home)
}

/// Searches for `shelf.toml` in common absolute locations
/// and then in the current directory and its ancestors.
///
/// Search order (platform-specific paths determined by the `directories` crate):
/// 1. `BaseDirs::config_dir()`/shelf/shelf.toml (e.g., `$XDG_CONFIG_HOME/shelf/shelf.toml` on Linux, `%APPDATA%\shelf\shelf.toml` on Windows)
/// 2. `BaseDirs::home_dir()`/shelf.toml
/// 3. `BaseDirs::home_dir()`/.shelf.toml
/// 4. Current directory and its ancestors: `shelf.toml`, `.shelf.toml`
pub(super) fn find_and_load_config() -> Result<Config> {
    // Build an iterator of candidate paths, starting with standard locations.
    let standard_paths = BaseDirs::new().into_iter().flat_map(|dirs| {
        [
            dirs.config_dir().join("shelf/shelf.toml"),
            dirs.home_dir().join("shelf.toml"),
            dirs.home_dir().join(".shelf.toml"),
        ]
    });

    // Chain it with paths from the current directory and its ancestors.
    let binding = std::env::current_dir()?;
    let ancestor_paths = binding
        .ancestors()
        .flat_map(|path| [path.join("shelf.toml"), path.join(".shelf.toml")]);

    let mut candidate_paths = standard_paths.chain(ancestor_paths);

    // Search through all candidate paths. `find_map` will stop at the first `Some`.
    let maybe_config_result = candidate_paths.find_map(|p| try_load_from(&p));

    // If a file was found, `maybe_config_result` is `Some(Result<Config>)`.
    // We transpose this to `Result<Option<Config>>` to handle the error case.
    // If no file was found, we return a default config.
    Ok(maybe_config_result.transpose()?.unwrap_or_default())
}

/// Attempts to load a `Config` from the given path.
///
/// If the file exists, it returns `Some(Result<Config>)`. The `Result` will be
/// `Ok` on successful parsing or `Err` on I/O or parse errors.
/// If the file does not exist, it returns `None`.
fn try_load_from(path: &Path) -> Option<Result<Config>> {
    if !path.exists() {
        return None;
    }

    println!("Loading config from: {}", path.display());
    let result = fs::read_to_string(path)
        .map_err(anyhow::Error::from)
        .and_then(|content| toml::from_str(&content).map_err(anyhow::Error::from));

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Create a uniquely named temporary directory under the system temp dir.
    fn make_temp_dir(prefix: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis();
        let dir = env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), now));
        fs::create_dir_all(&dir).expect("failed to create temp dir");
        dir
    }

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("failed to create parent dirs");
        }
        let mut file = File::create(path).expect("failed to create file");
        file.write_all(contents.as_bytes())
            .expect("failed to write file");
    }

    #[test]
    fn try_load_from_missing_file_returns_none() {
        let dir = make_temp_dir("shelf_test_missing");
        let p = dir.join("nonexistent.toml");
        assert!(!p.exists());
        assert!(try_load_from(&p).is_none());
    }

    #[test]
    fn try_load_from_invalid_toml_returns_err() {
        let dir = make_temp_dir("shelf_test_invalid");
        let p = dir.join("shelf.toml");
        write_file(&p, "this is : not = valid toml");
        let result = try_load_from(&p);
        assert!(result.is_some(), "expected Some(Result), got None");
        let inner = result.unwrap();
        assert!(inner.is_err(), "expected Err from invalid toml");
    }

    #[test]
    fn try_load_from_valid_toml_parses_config() {
        let dir = make_temp_dir("shelf_test_valid");
        let p = dir.join("shelf.toml");
        let toml = r#"
[prompt]
skip_directories = ["target", "node_modules"]
skip_files = ["README.md"]
"#;
        write_file(&p, toml);
        let result = try_load_from(&p);
        assert!(result.is_some(), "expected Some(Result), got None");
        let cfg = result.unwrap().expect("expected Ok(Config)");
        assert_eq!(cfg.prompt.skip_directories, vec!["target", "node_modules"]);
        assert_eq!(cfg.prompt.skip_files, vec!["README.md"]);
    }

    #[test]
    fn find_and_load_config_prefers_config_dir_shelf_toml() {
        // Setup config dir with shelf/shelf.toml
        let config_base = make_temp_dir("shelf_test_config_dir");
        let shelf_dir = config_base.join("shelf");
        fs::create_dir_all(&shelf_dir).expect("failed to create shelf dir");
        let config_file = shelf_dir.join("shelf.toml");
        let toml = r#"
[prompt]
skip_directories = ["from_config_dir"]
"#;
        write_file(&config_file, toml);

        // Set XDG_CONFIG_HOME to our temp config_base
        unsafe { env::set_var("XDG_CONFIG_HOME", &config_base) };
        // Ensure HOME doesn't also have a shelf.toml that would take precedence in tests
        let home_dir = make_temp_dir("shelf_test_no_home_config");
        unsafe { env::set_var("HOME", &home_dir) };

        // Use a current dir without shelf files
        let cwd = make_temp_dir("shelf_test_cwd_empty");
        env::set_current_dir(&cwd).expect("failed to set cwd");

        let cfg = find_and_load_config().expect("expected Ok(Config)");
        assert_eq!(cfg.prompt.skip_directories, vec!["from_config_dir"]);
    }

    #[test]
    fn find_and_load_config_falls_back_to_home_shelf_toml() {
        // No config_dir file, but create HOME/shelf.toml
        let config_base = make_temp_dir("shelf_test_config_dir_none");
        unsafe { env::set_var("XDG_CONFIG_HOME", &config_base) }; // empty

        let home_dir = make_temp_dir("shelf_test_home");
        let home_file = home_dir.join("shelf.toml");
        let toml = r#"
[prompt]
skip_directories = ["from_home"]
"#;
        write_file(&home_file, toml);
        unsafe { env::set_var("HOME", &home_dir) };

        let cwd = make_temp_dir("shelf_test_cwd_empty2");
        env::set_current_dir(&cwd).expect("failed to set cwd");

        let cfg = find_and_load_config().expect("expected Ok(Config)");
        assert_eq!(cfg.prompt.skip_directories, vec!["from_home"]);
    }

    #[test]
    fn find_and_load_config_checks_ancestor_directories() {
        // No standard config files, create a shelf.toml in an ancestor of cwd
        let base = make_temp_dir("shelf_test_ancestors");
        let ancestor = base.join("project");
        let nested = ancestor.join("src").join("inner");
        fs::create_dir_all(&nested).expect("failed to create nested dir");

        let ancestor_file = ancestor.join("shelf.toml");
        let toml = r#"
[prompt]
skip_directories = ["from_ancestor"]
"#;
        write_file(&ancestor_file, toml);

        // Ensure no XDG_CONFIG_HOME or HOME shelf files will be found
        let config_base = make_temp_dir("shelf_test_config_dir_empty");
        unsafe { env::set_var("XDG_CONFIG_HOME", &config_base) };
        let home_dir = make_temp_dir("shelf_test_home_empty");
        unsafe { env::set_var("HOME", &home_dir) };

        env::set_current_dir(&nested).expect("failed to set cwd");

        let cfg = find_and_load_config().expect("expected Ok(Config)");
        assert_eq!(cfg.prompt.skip_directories, vec!["from_ancestor"]);
    }

    #[test]
    fn find_and_load_config_returns_default_when_no_file() {
        // No files anywhere
        let config_base = make_temp_dir("shelf_test_config_dir_none2");
        unsafe { env::set_var("XDG_CONFIG_HOME", &config_base) };
        let home_dir = make_temp_dir("shelf_test_home_none");
        unsafe { env::set_var("HOME", &home_dir) };
        let cwd = make_temp_dir("shelf_test_cwd_none");
        env::set_current_dir(&cwd).expect("failed to set cwd");

        let cfg = find_and_load_config().expect("expected Ok(Config)");
        // default config should have empty vectors
        assert!(cfg.prompt.skip_directories.is_empty());
        assert!(cfg.prompt.skip_files.is_empty());
    }

    #[test]
    fn init_bare_repo_returns_dots_when_home_set() {
        // Set HOME to a temp dir and ensure init_bare_repo succeeds.
        let home_dir = make_temp_dir("shelf_test_init_home");
        unsafe { env::set_var("HOME", &home_dir) };
        // Also set XDG_CONFIG_HOME to avoid interference
        let config_base = make_temp_dir("shelf_test_init_config");
        unsafe { env::set_var("XDG_CONFIG_HOME", &config_base) };

        let res = init_bare_repo();
        assert!(
            res.is_ok(),
            "expected init_bare_repo to succeed, got {:?}",
            res
        );
    }
}
