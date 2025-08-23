use std::error::Error;
use std::fs;
use std::path::Path;

const SOURCE_ASSETS_DIR: &str = "assets/prompts";
const CONFIG_SUBPATH: &str = "shelf/assets/prompts";

fn main() -> Result<(), Box<dyn Error>> {
    let base_dirs = directories::BaseDirs::new()
        .ok_or("Could not find home directory to construct config path")?;
    let target_assets_dir = base_dirs.config_dir().join(CONFIG_SUBPATH);

    ensure_dir_exists(&target_assets_dir)?;
    copy_assets_from_source_to_target(SOURCE_ASSETS_DIR, &target_assets_dir)?;

    // Create a default shelf.toml config file if it doesn't exist.
    let shelf_config_dir = base_dirs.config_dir().join("shelf");
    create_default_config_if_not_exists(&shelf_config_dir)?;

    Ok(())
}

/// Creates a default `shelf.toml` in the given directory if one doesn't already exist.
fn create_default_config_if_not_exists(shelf_config_dir: &Path) -> Result<(), Box<dyn Error>> {
    let config_file_path = shelf_config_dir.join("shelf.toml");
    if config_file_path.exists() {
        return Ok(());
    }

    let default_config_content = r#"# Shelf configuration file.
# You can customize the behavior of `shelf prompt` command here.

[prompt]
# List of directory names to skip when collecting files.
skip_directories = [
    ".git",
    "target",
    "node_modules",
    "dist",
    "build",
    ".vscode",
    ".idea",
    "__pycache__",
]

# List of file names to skip.
skip_files = [
    "*.lock",
    "*.log",
    ".DS_Store",
]
"#;

    fs::write(&config_file_path, default_config_content)
        .map_err(|e| format!("Failed to write default config file to {config_file_path:?}: {e}"))?;

    println!(
        "cargo:warning=Created a default shelf config at: {}",
        config_file_path.display()
    );

    Ok(())
}

/// Ensures `dir` exists, creating it and any missing parents if necessary.
fn ensure_dir_exists(dir: &Path) -> Result<(), Box<dyn Error>> {
    if !dir.exists() {
        fs::create_dir_all(dir)
            .map_err(|e| format!("Failed to create assets directory {dir:?}: {e}"))?;
    }
    Ok(())
}

/// Copies regular files from `source_dir` (relative path) into `target_dir`.
/// Only files (not directories, symlinks, etc.) are copied.
fn copy_assets_from_source_to_target(
    source_dir: &str,
    target_dir: &Path,
) -> Result<(), Box<dyn Error>> {
    let entries = fs::read_dir(source_dir)
        .map_err(|e| format!("Failed to read source assets directory '{source_dir}': {e}",))?;

    for entry_res in entries {
        let entry = entry_res.map_err(|e| format!("Failed to read directory entry: {e}"))?;
        let file_type = entry
            .file_type()
            .map_err(|e| format!("Failed to get file type for {:?}: {}", entry.path(), e))?;

        if file_type.is_file() {
            let file_name = entry.file_name();
            let dest_path = target_dir.join(&file_name);
            fs::copy(entry.path(), &dest_path).map_err(|e| {
                format!(
                    "Failed to copy {:?} to {:?}: {}",
                    entry.path(),
                    dest_path,
                    e
                )
            })?;
        }
    }

    Ok(())
}
