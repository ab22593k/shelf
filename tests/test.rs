use anyhow::Result;
use shelf::SlfIndex;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tokio::fs;

async fn setup_test_environment() -> Result<(TempDir, SlfIndex)> {
    let temp_dir = TempDir::new()?;
    let config_dir = temp_dir.path().join("shelf");
    let target_dir = config_dir.join("dotfiles");
    fs::create_dir_all(&target_dir).await?;
    let index = SlfIndex::new(&target_dir).await?;
    Ok((temp_dir, index))
}

async fn create_test_file(dir: &Path, name: &str, content: &str) -> Result<PathBuf> {
    let file_path = dir.join(name);
    fs::write(&file_path, content).await?;
    Ok(file_path)
}

#[tokio::test]
async fn test_add_and_list_dotfile() -> Result<()> {
    let (temp_dir, mut index) = setup_test_environment().await?;
    let test_file = create_test_file(temp_dir.path(), ".testrc", "test content").await?;

    index.add_ref(&test_file).await?;

    let dotfiles = index.list().collect::<Vec<_>>();
    assert_eq!(dotfiles.len(), 1);
    assert_eq!(dotfiles[0].0, ".testrc");
    assert_eq!(dotfiles[0].1.source(), &test_file);

    Ok(())
}

#[tokio::test]
async fn test_remove_dotfile() -> Result<()> {
    let (temp_dir, mut index) = setup_test_environment().await?;
    let test_file = create_test_file(temp_dir.path(), ".testrc", "test content").await?;

    index.add_ref(&test_file).await?;
    index.remove_ref(".testrc")?;

    let dotfiles = index.list().collect::<Vec<_>>();
    assert_eq!(dotfiles.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_sync_dotfiles() -> Result<()> {
    let (temp_dir, mut index) = setup_test_environment().await?;
    let test_file1 = create_test_file(temp_dir.path(), ".testrc1", "test content 1").await?;
    let test_file2 = create_test_file(temp_dir.path(), ".testrc2", "test content 2").await?;

    index.add_ref(&test_file1).await?;
    index.add_ref(&test_file2).await?;

    index.do_sync().await?;

    let target_dir = temp_dir.path().join("shelf").join("dotfiles");
    assert!(target_dir.join(".testrc1").exists());
    assert!(target_dir.join(".testrc2").exists());

    Ok(())
}

#[tokio::test]
async fn test_add_nonexistent_file() -> Result<()> {
    let (temp_dir, mut index) = setup_test_environment().await?;
    let nonexistent_file = temp_dir.path().join("nonexistent");

    let result = index.add_ref(&nonexistent_file).await;
    assert!(result.is_err());

    Ok(())
}

#[tokio::test]
async fn test_remove_nonexistent_dotfile() -> Result<()> {
    let (_, mut index) = setup_test_environment().await?;

    let result = index.remove_ref("nonexistent");
    assert!(result.is_err());

    Ok(())
}
