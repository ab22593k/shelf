use directories::BaseDirs;
use std::fs;
use std::io;
use std::path::Path;

fn main() -> Result<(), io::Error> {
    let config_dir = BaseDirs::new()
        .map(|base| base.config_dir().join("shelf"))
        .expect("Could not create `shelf` config directory");

    // Your project-specific directory
    let target_dir = Path::new(&config_dir).join("assets");

    // Ensure the directory exists
    fs::create_dir_all(&target_dir)?;

    // Copy assets from your project's assets folder
    let assets_source = Path::new("assets");

    copy_dir_recursive(assets_source, &target_dir).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!(
                "Failed to copy directory '{}' to '{}': {}",
                assets_source.display(),
                target_dir.display(),
                e
            ),
        )
    })?;

    Ok(())
}

fn copy_dir_recursive(source: &Path, target: &Path) -> io::Result<()> {
    if !target.exists() {
        fs::create_dir_all(target)?;
    }

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let dest = target.join(entry.file_name());

        if path.is_dir() {
            copy_dir_recursive(&path, &dest)?;
        } else {
            fs::copy(&path, &dest).map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to copy '{}' to '{}': {}",
                        path.display(),
                        dest.display(),
                        e
                    ),
                )
            })?;
        }
    }
    Ok(())
}
