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

async fn create_test_files(dir: &Path, files: &[(&str, &str)]) -> Result<Vec<PathBuf>> {
    let mut created_files = Vec::new();
    for (name, content) in files {
        let file_path = create_test_file(dir, name, content).await?;
        created_files.push(file_path);
    }
    Ok(created_files)
}

async fn add_test_files_to_index(index: &mut SlfIndex, files: &[PathBuf]) -> Result<()> {
    for file in files {
        index.add_ref(file.to_str().unwrap()).await?;
    }
    Ok(())
}

async fn setup_test_index_with_files(
    files: &[(&str, &str)],
) -> Result<(TempDir, SlfIndex, Vec<PathBuf>)> {
    let (temp_dir, mut index) = setup_test_environment().await?;
    let created_files = create_test_files(temp_dir.path(), files).await?;
    add_test_files_to_index(&mut index, &created_files).await?;
    Ok((temp_dir, index, created_files))
}
#[tokio::test]
async fn test_add_and_list_dotfile() -> Result<()> {
    let mut files = [
        (".testrc", "test content"),
        (".vimrc", "set nocompatible"),
        (".bashrc", "export PATH=$PATH:/usr/local/bin"),
    ];
    files.sort_by(|a, b| a.0.cmp(b.0));
    let (temp_dir, mut index, created_files) = setup_test_index_with_files(&files).await?;

    let mut dotfiles: Vec<_> = index.list().collect();
    dotfiles.sort_by(|a, b| a.0.cmp(b.0));

    assert_eq!(
        dotfiles.len(),
        files.len(),
        "Expected {} dotfiles, found {}",
        files.len(),
        dotfiles.len()
    );

    for (i, (name, dotfile)) in dotfiles.iter().enumerate() {
        let expected_name = files[i].0;
        assert!(
            name.ends_with(expected_name),
            "Unexpected dotfile name: {}, expected to end with: {}",
            name,
            expected_name
        );

        let canonical_test_file = created_files[i]
            .canonicalize()
            .map_err(|e| anyhow::anyhow!("Failed to canonicalize test file: {}", e))?;
        let canonical_dotfile_source = dotfile
            .source()
            .canonicalize()
            .map_err(|e| anyhow::anyhow!("Failed to canonicalize dotfile source: {}", e))?;

        assert_eq!(
            canonical_dotfile_source, canonical_test_file,
            "Paths don't match for {}: {:?} vs {:?}",
            name, canonical_dotfile_source, canonical_test_file
        );

        // Verify file content
        let content = fs::read_to_string(dotfile.source())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read dotfile content: {}", e))?;
        assert_eq!(
            content, files[i].1,
            "Content mismatch for {}: expected '{}', got '{}'",
            name, files[i].1, content
        );
    }

    // Test adding a file that already exists
    let result = index.add_ref(created_files[0].to_str().unwrap()).await;
    assert!(result.is_err(), "Adding an existing dotfile should fail");

    // Cleanup
    drop(index);
    assert!(
        temp_dir.path().exists(),
        "Temporary directory should still exist"
    );
    temp_dir
        .close()
        .map_err(|e| anyhow::anyhow!("Failed to close temporary directory: {}", e))?;

    Ok(())
}

#[tokio::test]
async fn test_add_multiple_dotfiles() -> Result<()> {
    let files = [
        (".bashrc", "# Bash configuration"),
        (".vimrc", "\" Vim configuration"),
        (".gitconfig", "[user]\n\tname = Test User"),
    ];
    let (_, index, _) = setup_test_index_with_files(&files).await?;

    let dotfiles = index.list().collect::<Vec<_>>();
    assert_eq!(
        dotfiles.len(),
        3,
        "Expected 3 dotfiles, found {}",
        dotfiles.len()
    );

    Ok(())
}
#[tokio::test]
async fn test_add_duplicate_dotfile() -> Result<()> {
    let (temp_dir, mut index) = setup_test_environment().await?;
    let test_file = create_test_file(temp_dir.path(), ".testrc", "original content").await?;

    index.add_ref(test_file.to_str().unwrap()).await?;
    let result = index.add_ref(test_file.to_str().unwrap()).await;

    match result {
        Err(e) => {
            // Check for a more general error condition
            assert!(
                e.to_string().contains("already") || e.to_string().contains("exist"),
                "Expected error related to duplicate file, got: {}",
                e
            );
        }
        Ok(_) => {
            // Use assert! instead of panic! for better test output
            assert!(false, "Adding a duplicate dotfile should fail");
        }
    }

    // Verify that the original file still exists and hasn't been modified
    assert!(test_file.exists(), "Original file should still exist");
    let content = fs::read_to_string(&test_file).await?;
    assert_eq!(
        content, "original content",
        "Original file content should be unchanged"
    );

    Ok(())
}
#[tokio::test]
async fn test_sync_creates_symlinks() -> Result<()> {
    let files = [(".testrc", "test content")];
    let (temp_dir, index, created_files) = setup_test_index_with_files(&files).await?;

    index.do_sync().await?;

    let target_dir = temp_dir.path().join("shelf").join("dotfiles");
    let symlink = target_dir.join(".testrc");

    assert!(symlink.exists(), "Symlink should exist after sync");
    assert!(symlink.is_symlink(), "Created file should be a symlink");

    let link_target = std::fs::read_link(&symlink)?;
    let canonical_link_target = link_target.canonicalize()?;
    let canonical_created_file = created_files[0].canonicalize()?;
    assert_eq!(
        canonical_link_target, canonical_created_file,
        "Symlink should point to the original file"
    );

    // Verify original file still exists and contains the correct content
    assert!(
        created_files[0].exists(),
        "Original file should still exist"
    );
    let original_content = fs::read_to_string(&created_files[0]).await?;
    assert_eq!(
        original_content, "test content",
        "Original file content should be unchanged"
    );

    // Verify symlink content matches original file
    let symlink_content = fs::read_to_string(&symlink).await?;
    assert_eq!(
        symlink_content, "test content",
        "Symlink content should match original file"
    );

    Ok(())
}
#[tokio::test]
async fn test_list_returns_correct_info() -> Result<()> {
    let mut files = [
        (".bashrc", "# Bash configuration"),
        (".vimrc", "\" Vim configuration"),
        (".gitconfig", "[user]\n\tname = Test User"),
    ];
    files.sort_by(|a, b| a.0.cmp(b.0));
    let (temp_dir, index, created_files) = setup_test_index_with_files(&files).await?;

    let mut dotfiles: Vec<_> = index.list().collect();
    dotfiles.sort_by(|a, b| a.0.cmp(b.0));

    assert_eq!(
        dotfiles.len(),
        files.len(),
        "Expected {} dotfiles, found {}",
        files.len(),
        dotfiles.len()
    );

    for (i, (name, dotfile)) in dotfiles.iter().enumerate() {
        let expected_name = files[i].0;
        assert!(
            name.ends_with(expected_name),
            "Unexpected dotfile name: {}, expected to end with: {}",
            name,
            expected_name
        );

        let canonical_source = dotfile.source().canonicalize()?;
        let canonical_created = created_files[i].canonicalize()?;
        assert_eq!(
            canonical_source, canonical_created,
            "Paths don't match for {}: {:?} vs {:?}",
            name, canonical_source, canonical_created
        );

        // Verify file content
        let content = fs::read_to_string(dotfile.source()).await?;
        assert_eq!(
            content, files[i].1,
            "Content mismatch for {}: expected '{}', got '{}'",
            name, files[i].1, content
        );
    }

    // Test that all expected files are present
    let listed_names: Vec<_> = dotfiles.iter().map(|(name, _)| name).collect();
    for file in &files {
        assert!(
            listed_names.iter().any(|&name| name.ends_with(file.0)),
            "Expected file {} not found in listed dotfiles",
            file.0
        );
    }

    // Cleanup check
    drop(index);
    assert!(
        temp_dir.path().exists(),
        "Temporary directory should still exist"
    );
    temp_dir.close()?;

    Ok(())
}

#[tokio::test]
async fn test_remove_dotfile() -> Result<()> {
    let (temp_dir, mut index) = setup_test_environment().await?;
    let test_file = create_test_file(temp_dir.path(), ".testrc", "test content").await?;
    index.add_ref(test_file.to_str().unwrap()).await?;
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

    index.add_ref(&test_file1.to_str().unwrap()).await?;
    index.add_ref(&test_file2.to_str().unwrap()).await?;

    index.do_sync().await?;

    let target_dir = temp_dir.path().join("shelf").join("dotfiles");
    assert!(target_dir.join(".testrc1").exists());
    assert!(target_dir.join(".testrc2").exists());

    // Verify original files still exist and contain the correct content
    assert!(test_file1.exists(), "Original file 1 should still exist");
    assert!(test_file2.exists(), "Original file 2 should still exist");

    let content1 = fs::read_to_string(&test_file1).await?;
    assert_eq!(
        content1, "test content 1",
        "Original file 1 content should be unchanged"
    );

    let content2 = fs::read_to_string(&test_file2).await?;
    assert_eq!(
        content2, "test content 2",
        "Original file 2 content should be unchanged"
    );

    // Verify symlink content matches original files
    let symlink1_content = fs::read_to_string(target_dir.join(".testrc1")).await?;
    assert_eq!(
        symlink1_content, "test content 1",
        "Symlink 1 content should match original file"
    );

    let symlink2_content = fs::read_to_string(target_dir.join(".testrc2")).await?;
    assert_eq!(
        symlink2_content, "test content 2",
        "Symlink 2 content should match original file"
    );

    Ok(())
}

#[tokio::test]
async fn test_add_nonexistent_file() -> Result<()> {
    let (temp_dir, mut index) = setup_test_environment().await?;
    let nonexistent_file = temp_dir.path().join("nonexistent");

    let result = index.add_ref(&nonexistent_file.to_str().unwrap()).await;
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
