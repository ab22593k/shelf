use anyhow::Result;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tokio::fs;

use shlf::dotfile::Dotfiles;
async fn setup_test_environment() -> Result<(TempDir, Dotfiles)> {
    let temp_dir = TempDir::new()?;
    let config_dir = temp_dir.path().join("shelf");
    let target_dir = config_dir.join("dotfiles");
    fs::create_dir_all(&target_dir).await?;
    let absolute_target_dir = fs::canonicalize(&target_dir).await?;

    // Ensure the target directory is accessible
    fs::metadata(&absolute_target_dir).await?;

    let index = Dotfiles::new(absolute_target_dir).await?;
    Ok((temp_dir, index))
}

async fn create_test_file(dir: &Path, name: &str, content: &str) -> Result<PathBuf> {
    let file_path = dir.join(name);
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).await?;
    }
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

async fn add_test_files_to_index(index: &mut Dotfiles, files: &[PathBuf]) -> Result<()> {
    for file in files {
        index.add(file.to_str().unwrap()).await?;
    }
    Ok(())
}
async fn setup_test_index_with_files(
    files: &[(&str, &str)],
) -> Result<(TempDir, Dotfiles, Vec<PathBuf>)> {
    let (temp_dir, mut df) = setup_test_environment().await?;
    let created_files = create_test_files(temp_dir.path(), files).await?;
    add_test_files_to_index(&mut df, &created_files).await?;
    Ok((temp_dir, df, created_files))
}

#[tokio::test]
async fn test_add_and_list_dotfile() -> Result<()> {
    let mut files = [
        (".testrc", "test content"),
        (".vimrc", "set nocompatible"),
        (".bashrc", "export PATH=$PATH:/usr/local/bin"),
    ];
    files.sort_by(|a, b| a.0.cmp(b.0));
    let (temp_dir, mut df, created_files) = setup_test_index_with_files(&files).await?;
    // let mut dotfiles = index;
    let mut dotfiles: Vec<_> = df.dotfiles.iter().collect();
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
            .get_source()
            .canonicalize()
            .map_err(|e| anyhow::anyhow!("Failed to canonicalize dotfile source: {}", e))?;

        assert_eq!(
            canonical_dotfile_source, canonical_test_file,
            "Paths don't match for {}: {:?} vs {:?}",
            name, canonical_dotfile_source, canonical_test_file
        );

        // Verify file content
        let content = fs::read_to_string(dotfile.get_source())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read dotfile content: {}", e))?;
        assert_eq!(
            content, files[i].1,
            "Content mismatch for {}: expected '{}', got '{}'",
            name, files[i].1, content
        );
    }
    // Test adding a file that already exists
    let file_path = created_files[0].to_str().unwrap().to_string();
    let result = df.add(&file_path).await;
    assert!(result.is_ok(), "Adding an existing dotfile should succeed");

    // Cleanup
    drop(df);
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
    let (temp_dir, mut index) = setup_test_environment().await?;

    let files = [
        (".bashrc", "# Bash configuration"),
        (".vimrc", "\" Vim configuration"),
        (".gitconfig", "[user]\n\tname = Test User"),
    ];
    let created_files = create_test_files(temp_dir.path(), &files).await?;

    let results = index
        .add_multi(created_files.iter().map(|p| p.to_str().unwrap()))
        .await;

    assert_eq!(
        results.len(),
        files.len(),
        "Expected {} results, got {}",
        files.len(),
        results.len()
    );
    for result in &results {
        assert!(result.is_ok(), "Adding a dotfile failed: {:?}", result);
    }

    let dotfiles: Vec<_> = index.dotfiles.iter().collect();
    assert_eq!(
        dotfiles.len(),
        files.len(),
        "Expected {} dotfiles, found {}",
        files.len(),
        dotfiles.len()
    );

    Ok(())
}
#[tokio::test]
async fn test_add_duplicate_dotfile() -> Result<()> {
    let (temp_dir, mut index) = setup_test_environment().await?;
    let test_file = create_test_file(temp_dir.path(), ".testrc", "original content").await?;

    index.add(test_file.to_str().unwrap()).await?;
    let result = index.add(test_file.to_str().unwrap()).await;

    assert!(result.is_ok(), "Adding a duplicate dotfile should succeed");

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
async fn test_add_multiple_dotfiles_at_once() -> Result<()> {
    let (temp_dir, mut index) = setup_test_environment().await?;
    let files = [
        (".bashrc", "# Bash configuration"),
        (".vimrc", "\" Vim configuration"),
        (".gitconfig", "[user]\n\tname = Test User"),
    ];
    let created_files = create_test_files(temp_dir.path(), &files).await?;

    let results = index
        .add_multi(created_files.iter().map(|p| p.to_str().unwrap()))
        .await;

    assert_eq!(results.len(), 3, "Expected 3 results, one for each file");
    assert!(
        results.iter().all(|r| r.is_ok()),
        "All files should be added successfully"
    );

    let dotfiles = index.dotfiles.iter().collect::<Vec<_>>();

    assert_eq!(dotfiles.len(), 3, "Expected 3 dotfiles to be tracked");

    for (file, content) in files.iter() {
        let dotfile = dotfiles.iter().find(|(name, _)| name.ends_with(file));
        assert!(dotfile.is_some(), "Dotfile {} should be tracked", file);
        let (_, entry) = dotfile.unwrap();
        let file_content = fs::read_to_string(entry.get_source()).await?;
        assert_eq!(
            file_content, *content,
            "File content should match for {}",
            file
        );
    }

    Ok(())
}
#[tokio::test]
async fn test_link_creates_copies() -> Result<()> {
    let files = [(".testrc", "test content")];
    let (temp_dir, index, created_files) = setup_test_index_with_files(&files).await?;
    let source_file = created_files[0].canonicalize()?;
    let target_dir = temp_dir
        .path()
        .join("shelf")
        .join("dotfiles")
        .canonicalize()?;
    let source_file = created_files[0].canonicalize()?;
    let target_dir = temp_dir
        .path()
        .join("shelf")
        .join("dotfiles")
        .canonicalize()?;
    index.copy().await?;

    let target_dir = temp_dir.path().join("shelf").join("dotfiles");
    let copied_file = target_dir.join(".testrc");

    assert!(copied_file.exists(), "Copie file should exist after sync");
    assert!(
        !copied_file.is_symlink(),
        "Created file should not be a symlink"
    );

    let canonical_copied_file = copied_file.canonicalize()?;
    let canonical_created_file = created_files[0].canonicalize()?;
    assert_ne!(
        canonical_copied_file, canonical_created_file,
        "Copied file should not be the same as the original file"
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

    // Verify copied file content matches original file
    let copied_content = fs::read_to_string(&copied_file).await?;
    assert_eq!(
        copied_content, "test content",
        "Copied file content should match original file"
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

    let mut dotfiles: Vec<_> = index.dotfiles.iter().collect();
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
        let canonical_source = dotfile.get_source().canonicalize()?;
        let canonical_created = created_files[i].canonicalize()?;
        assert_eq!(
            canonical_source, canonical_created,
            "Paths don't match for {}: {:?} vs {:?}",
            name, canonical_source, canonical_created
        );

        // Verify file content
        let content = fs::read_to_string(dotfile.get_source()).await?;
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
    index.add(test_file.to_str().unwrap()).await?;
    let remove_results = index.remove_multi(&[".testrc"]);
    assert!(
        remove_results.iter().all(|r| r.is_ok()),
        "Removing dotfile should succeed"
    );

    let dotfiles = index.dotfiles.iter().collect::<Vec<_>>();
    assert_eq!(dotfiles.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_remove_multiple_dotfiles() -> Result<()> {
    let (temp_dir, mut index) = setup_test_environment().await?;
    let files = [
        (".testrc1", "test content 1"),
        (".testrc2", "test content 2"),
        (".testrc3", "test content 3"),
    ];
    let created_files = create_test_files(temp_dir.path(), &files).await?;
    add_test_files_to_index(&mut index, &created_files).await?;

    // Remove two of the three dotfiles
    let result = index.remove_multi(&[".testrc1", ".testrc2"]);
    assert!(
        result.iter().all(|r| r.is_ok()),
        "Removing multiple dotfiles should succeed"
    );

    // Get the remaining dotfiles without cloning
    let remaining_dotfiles = index.dotfiles;
    assert_eq!(
        remaining_dotfiles.len(),
        1,
        "Should have one remaining dotfile"
    );
    assert!(
        remaining_dotfiles.contains_key(".testrc3"),
        "The remaining dotfile should be .testrc3"
    );

    Ok(())
}

#[tokio::test]
async fn test_remove_nonexistent_dotfiles() -> Result<()> {
    let (temp_dir, mut index) = setup_test_environment().await?;
    let test_file = create_test_file(temp_dir.path(), ".testrc", "test content").await?;
    index.add(test_file.to_str().unwrap()).await?;
    let result = index.remove_multi(&[".testrc", "nonexistent1", "nonexistent2"]);

    assert_eq!(result.len(), 3, "Expected three results");
    assert!(
        result[0].is_ok(),
        "Removing existing dotfile should succeed"
    );
    assert!(result[1].is_err(), "Removing nonexistent1 should fail");
    assert!(result[2].is_err(), "Removing nonexistent2 should fail");

    let error_messages: Vec<String> = result
        .iter()
        .filter_map(|r| r.as_ref().err().map(|e| e.to_string().to_lowercase()))
        .collect();

    assert!(
        error_messages
            .iter()
            .any(|msg| msg.contains("not found") || msg.contains("doesn't exist")),
        "At least one error message should indicate that a dotfile was not found"
    );

    let remaining_dotfiles = index.dotfiles;
    assert_eq!(
        remaining_dotfiles.len(),
        0,
        "All existing dotfiles should be removed"
    );

    Ok(())
}

#[tokio::test]
async fn test_remove_dotfiles_partial_success() -> Result<()> {
    let (temp_dir, mut index) = setup_test_environment().await?;
    let files = [
        (".testrc1", "test content 1"),
        (".testrc2", "test content 2"),
    ];
    let created_files = create_test_files(temp_dir.path(), &files).await?;
    add_test_files_to_index(&mut index, &created_files).await?;
    let result = index.remove_multi(&[".testrc1", ".testrc2", "nonexistent"]);

    assert_eq!(result.len(), 3, "Expected three results");
    assert!(result[0].is_ok(), "Removing .testrc1 should succeed");
    assert!(result[1].is_ok(), "Removing .testrc2 should succeed");
    assert!(result[2].is_err(), "Removing nonexistent should fail");

    let error_messages: Vec<String> = result
        .iter()
        .filter_map(|r| r.as_ref().err().map(|e| e.to_string().to_lowercase()))
        .collect();

    assert!(
        error_messages
            .iter()
            .any(|msg| msg.contains("not found") || msg.contains("doesn't exist")),
        "Error message should indicate that a dotfile was not found"
    );

    let remaining_dotfiles = index.dotfiles;
    assert_eq!(
        remaining_dotfiles.len(),
        0,
        "All existing dotfiles should be removed"
    );

    Ok(())
}
#[tokio::test]
async fn test_sync_dotfiles() -> Result<()> {
    let (temp_dir, mut index) = setup_test_environment().await?;
    let test_file1 = create_test_file(temp_dir.path(), ".testrc1", "test content 1").await?;
    let test_file2 = create_test_file(temp_dir.path(), ".testrc2", "test content 2").await?;
    let test_file3 = create_test_file(temp_dir.path(), ".testrc3", "test content 3").await?;

    index.add(test_file1.to_str().unwrap()).await?;
    index.add(test_file2.to_str().unwrap()).await?;
    index.add(test_file3.to_str().unwrap()).await?;

    index.copy().await?;

    let target_dir = temp_dir.path().join("shelf").join("dotfiles");

    for i in 1..=3 {
        let file_name = format!(".testrc{}", i);
        let copied_file = target_dir.join(&file_name);
        assert!(
            copied_file.exists(),
            "Copied file {} should exist",
            file_name
        );
        assert!(
            !copied_file.is_symlink(),
            "Copied file {} should not be a symlink",
            file_name
        );

        let content = fs::read_to_string(&copied_file).await?;
        assert_eq!(
            content,
            format!("test content {}", i),
            "Copied file {} content should match original",
            file_name
        );
    }
    let test_file2 = create_test_file(temp_dir.path(), ".testrc2", "test content 2").await?;

    index.add(&test_file1.to_str().unwrap()).await?;
    index.add(&test_file2.to_str().unwrap()).await?;

    index.copy().await?;

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

    let result = index.add(&nonexistent_file.to_str().unwrap()).await;
    assert!(result.is_err());

    Ok(())
}

#[tokio::test]
async fn test_remove_nonexistent_dotfile() -> Result<()> {
    let (_, mut index) = setup_test_environment().await?;
    let result = index.remove_multi(&["nonexistent"]);

    assert_eq!(result.len(), 1, "Expected one result");
    assert!(
        result[0].is_err(),
        "Removing nonexistent dotfile should fail"
    );

    let error_message = result[0].as_ref().unwrap_err().to_string().to_lowercase();
    assert!(
        error_message.contains("not found") || error_message.contains("doesn't exist"),
        "Error message should indicate the dotfile was not found"
    );

    Ok(())
}
#[tokio::test]
async fn test_file_conflict_handling() -> Result<()> {
    let (temp_dir, mut index) = setup_test_environment().await?;
    let test_file = create_test_file(temp_dir.path(), ".testrc", "original content").await?;

    // Create a conflicting file in the target directory
    let target_dir = temp_dir.path().join("shelf").join("dotfiles");
    fs::create_dir_all(&target_dir).await?;
    let conflicting_file = target_dir.join(".testrc");
    fs::write(&conflicting_file, "conflicting content").await?;

    // Add the original file to the index
    index.add(test_file.to_str().unwrap()).await?;

    // Perform the copy operation
    index.copy().await?;

    // Verify that the file exists and contains the correct content
    let copied_file = target_dir.join(".testrc");
    assert!(copied_file.exists(), "Copied file should exist");
    assert!(
        !copied_file.is_symlink(),
        "Copied file should not be a symlink"
    );

    let copied_content = fs::read_to_string(&copied_file).await?;
    assert_eq!(
        copied_content, "original content",
        "Copied file should contain the original content, overwriting the conflicting content"
    );

    // Add the original file to the index
    index.add(test_file.to_str().unwrap()).await?;

    // Create a conflicting file in the target directory
    let target_dir = temp_dir.path().join("shelf").join("dotfiles");
    fs::create_dir_all(&target_dir).await?;
    let conflicting_file = target_dir.join(".testrc");
    fs::write(&conflicting_file, "conflicting content").await?;

    // Perform the copy operation
    index.copy().await?;

    // Verify that the file exists and contains the correct content
    let copied_file = target_dir.join(".testrc");
    assert!(copied_file.exists(), "Copied file should exist");
    assert!(
        !copied_file.is_symlink(),
        "Copied file should not be a symlink"
    );

    let copied_content = fs::read_to_string(&copied_file).await?;
    assert_eq!(
        copied_content, "original content",
        "Copied file should contain the original content, overwriting the conflicting content"
    );

    // Verify that the original file remains unchanged
    let original_content = fs::read_to_string(&test_file).await?;
    assert_eq!(
        original_content, "original content",
        "Original file content should remain unchanged"
    );

    // Add the dotfile to the index
    index.add(test_file.to_str().unwrap()).await?;

    // Create a conflicting file in the target directory
    let target_dir = temp_dir.path().join("shelf").join("dotfiles");
    fs::create_dir_all(&target_dir).await?;
    let conflicting_file = target_dir.join(".testrc");
    fs::write(&conflicting_file, "conflicting content").await?;

    // Copy dotfiles, which should overwrite the conflicting file
    index.copy().await?;

    // Verify that the file exists and contains the correct content
    let copied_file = target_dir.join(".testrc");
    assert!(copied_file.exists(), "Copied file should exist");
    assert!(
        !copied_file.is_symlink(),
        "Copied file should not be a symlink"
    );

    let copied_content = fs::read_to_string(&copied_file).await?;
    assert_eq!(
        copied_content, "original content",
        "Copied file should contain the original content, overwriting the conflicting content"
    );

    Ok(())
}

#[tokio::test]
async fn test_update_existing_dotfile() -> Result<()> {
    let (temp_dir, mut index) = setup_test_environment().await?;
    let initial_file = create_test_file(temp_dir.path(), ".testrc", "initial content").await?;
    index.add(initial_file.to_str().unwrap()).await?;

    let updated_file = create_test_file(temp_dir.path(), ".testrc", "updated content").await?;
    index.add(updated_file.to_str().unwrap()).await?;

    let dotfiles: Vec<_> = index.dotfiles.iter().collect();
    assert_eq!(dotfiles.len(), 1, "Should still have only one dotfile");

    let (_, entry) = dotfiles[0];
    let content = fs::read_to_string(&entry.source).await?;
    assert_eq!(content, "updated content", "Content should be updated");

    Ok(())
}

#[tokio::test]
async fn test_link_nested_dotfiles() -> Result<()> {
    let (temp_dir, mut index) = setup_test_environment().await?;
    let target_dir = index.target_directory.clone();

    // Create a deeply nested directory structure
    let nested_dir = temp_dir.path().join("level1").join("level2").join("level3");
    fs::create_dir_all(&nested_dir).await?;

    // Create files at different levels of nesting
    let root_file = create_test_file(temp_dir.path(), ".rootrc", "root content").await?;
    let level1_file = create_test_file(
        &temp_dir.path().join("level1"),
        ".level1rc",
        "level1 content",
    )
    .await?;
    let level3_file = create_test_file(&nested_dir, ".level3rc", "level3 content").await?;

    // Add all files to the index
    index.add(root_file.to_str().unwrap()).await?;
    index.add(level1_file.to_str().unwrap()).await?;
    index.add(level3_file.to_str().unwrap()).await?;

    // Perform the copy operation
    index.copy().await?;

    // Verify that all files are copied correctly
    assert!(
        target_dir.join(".rootrc").exists(),
        "Root file should exist"
    );
    assert!(
        target_dir.join(".level1rc").exists(),
        "Level 1 file should exist"
    );
    assert!(
        target_dir.join(".level3rc").exists(),
        "Level 3 file should exist"
    );

    // Create a new nested file
    let nested_file = create_test_file(&nested_dir, ".nestedrc", "nested content").await?;
    index.add(nested_file.to_str().unwrap()).await?;

    // Perform another copy operation
    index.copy().await?;

    // Verify that the new nested file is copied correctly
    let copied_nested_file = target_dir.join(".nestedrc");
    assert!(
        copied_nested_file.exists(),
        "Copied nested file should exist"
    );
    assert!(
        !copied_nested_file.is_symlink(),
        "Copied nested file should not be a symlink"
    );
    let content = fs::read_to_string(&copied_nested_file).await?;
    assert_eq!(
        content, "nested content",
        "Copied nested file content should match"
    );

    Ok(())
}

#[tokio::test]
async fn test_handle_broken_symlinks() -> Result<()> {
    let (temp_dir, mut index) = setup_test_environment().await?;
    let test_file = create_test_file(temp_dir.path(), ".testrc", "test content").await?;
    let target_dir = temp_dir.path().join("shelf").join("dotfiles");
    fs::create_dir_all(&target_dir).await?;

    // Create a broken symlink
    let broken_symlink = target_dir.join(".testrc");
    std::os::unix::fs::symlink("/nonexistent/path", &broken_symlink)?;

    // Add the original file to the index
    index.add(test_file.to_str().unwrap()).await?;

    // Perform the copy operation
    index.copy().await?;

    // Verify that the broken symlink has been replaced with the correct file
    assert!(broken_symlink.exists(), "Copied file should exist");
    assert!(
        !broken_symlink.is_symlink(),
        "Copied file should not be a symlink"
    );
    let content = fs::read_to_string(&broken_symlink).await?;
    assert_eq!(
        content, "test content",
        "Copied file should contain the correct content"
    );

    // Create an existing file (non-symlink) in the target directory
    let existing_file = target_dir.join(".existingrc");
    fs::write(&existing_file, "existing content").await?;

    // Add a new file to replace the existing one
    let new_file = create_test_file(temp_dir.path(), ".existingrc", "new content").await?;
    index.add(new_file.to_str().unwrap()).await?;

    // Perform another copy operation
    index.copy().await?;

    // Verify that the existing file has been replaced with the new content
    assert!(existing_file.exists(), "Existing file should still exist");
    assert!(
        !existing_file.is_symlink(),
        "Existing file should not be a symlink"
    );
    let content = fs::read_to_string(&existing_file).await?;
    assert_eq!(
        content, "new content",
        "Existing file should be updated with new content"
    );

    Ok(())
}
