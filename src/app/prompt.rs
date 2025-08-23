use anyhow::Result;
use clap::Parser;
use git2::Repository;
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use walkdir::WalkDir;

use crate::config::{Config, find_and_load_config};

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct PromptCMD {
    /// GitHub repository URL or local directory path
    #[arg(num_args = 1..)]
    repo_sources: Vec<String>,

    /// Output file path (optional)
    #[arg(short, long)]
    output_file: Option<String>,

    /// Maximum file size in bytes (default: 1MB)
    #[arg(long, default_value_t = 1024 * 1024)]
    max_size: u64,
}

/// A converter for GitHub repositories or local directories to an LLM-friendly text file.
struct RepoConverter {
    max_file_size: u64,
    config: Config,
}

// An enum to represent our file system tree structure.
// This solves the "cyclic type" error by defining a recursive structure correctly.
enum FileTreeNode {
    File(u64), // Contains the file size
    Directory(BTreeMap<String, FileTreeNode>),
}

impl RepoConverter {
    fn new(max_file_size: u64, config: Config) -> Self {
        Self {
            max_file_size,
            config,
        }
    }

    /// Clones a Git repository to a temporary directory.
    fn clone_repository(&self, url: &str, path: &Path) -> Result<(), git2::Error> {
        println!("Cloning repository: {url}");
        Repository::clone(url, path)?;
        Ok(())
    }

    /// Checks if a file should be skipped based on its name or size.
    fn should_skip_file(&self, file_path: &Path) -> bool {
        let mut skip_files: HashSet<&str> = [
            ".gitignore",
            ".gitattributes",
            ".gitmodules",
            ".gitkeep",
            ".dockerignore",
            ".npmignore",
            ".eslintignore",
            ".prettierignore",
            "thumbs.db",
            ".ds_store",
            "desktop.ini",
            "*.swp",
            "*.swo",
            "*~",
            ".env.local",
            ".env.development",
            ".env.production",
            ".env.test",
            "*.png",
            "*.svg",
        ]
        .iter()
        .cloned()
        .collect();

        // Add custom patterns from config
        for pattern in &self.config.prompt.skip_files {
            skip_files.insert(pattern);
        }

        if let Some(file_name) = file_path.file_name().and_then(|n| n.to_str())
            && skip_files.contains(file_name.to_lowercase().as_str())
        {
            return true;
        }

        if let Ok(metadata) = fs::metadata(file_path)
            && metadata.len() > self.max_file_size
        {
            return true;
        }

        false
    }

    /// Checks if a directory should be skipped.
    fn should_skip_directory(&self, dir_path: &Path) -> bool {
        let mut skip_directories: HashSet<&str> = [
            ".git",
            ".svn",
            ".hg",
            "__pycache__",
            ".pytest_cache",
            ".mypy_cache",
            "node_modules",
            ".npm",
            ".yarn",
            "bower_components",
            "vendor",
            "deps",
            "build",
            "dist",
            "target",
            "bin",
            "obj",
            "out",
            ".gradle",
            ".mvn",
            ".idea",
            ".vscode",
            ".vs",
            ".settings",
            ".eclipse",
            ".metadata",
            "venv",
            "env",
            ".env",
            "virtualenv",
            ".virtualenv",
            "conda-env",
            "coverage",
            ".coverage",
            ".nyc_output",
            "htmlcov",
            "test-results",
            "logs",
            "tmp",
            "temp",
            ".tmp",
            ".temp",
            "cache",
            ".cache",
        ]
        .iter()
        .cloned()
        .collect();

        for pattern in &self.config.prompt.skip_directories {
            skip_directories.insert(pattern);
        }

        if let Some(dir_name) = dir_path.file_name().and_then(|n| n.to_str()) {
            return skip_directories.contains(dir_name.to_lowercase().as_str());
        }

        false
    }

    /// Determines if a file is a text file.
    fn is_text_file(&self, file_path: &Path) -> bool {
        // A simplified check, for a more robust solution, consider using a crate like `content_inspector`.
        if let Ok(mut file) = File::open(file_path) {
            let mut buffer = [0; 1024];
            if let Ok(n) = file.read(&mut buffer) {
                return std::str::from_utf8(&buffer[..n]).is_ok();
            }
        }
        false
    }

    /// Collects all relevant files from a directory.
    fn collect_files(&self, repo_path: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for entry in WalkDir::new(repo_path)
            .into_iter()
            .filter_entry(|e| !self.should_skip_directory(e.path()))
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file()
                && !self.should_skip_file(entry.path())
                && self.is_text_file(entry.path())
            {
                files.push(entry.into_path());
            }
        }
        files.sort();
        files
    }

    /// Generates the LLM-friendly text output.
    fn generate_llm_friendly_text(
        &self,
        repo_path: &Path,
        files: &[PathBuf],
        source_identifier: &str,
    ) -> String {
        let mut output = String::new();

        // --- Header ---
        output.push_str(&"=".repeat(80));
        output.push('\n');
        output.push_str(&format!("REPOSITORY: {source_identifier}\n"));
        output.push_str(&format!(
            "CONVERTED: {}\n",
            std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
        ));
        output.push_str(&"=".repeat(80));
        output.push_str("\n\n");

        // --- Repository Structure ---
        output.push_str("REPOSITORY STRUCTURE:\n");
        output.push_str(&"-".repeat(40));
        output.push('\n');

        // Build the tree using the FileTreeNode enum
        let mut root = FileTreeNode::Directory(BTreeMap::new());

        // Helper recursive inserter to avoid holding multiple mutable borrows across iterations
        fn insert_path(node: &mut FileTreeNode, components: &[String], size: u64) {
            if let FileTreeNode::Directory(children) = node
                && let Some((name, rest)) = components.split_first()
            {
                if rest.is_empty() {
                    children.insert(name.clone(), FileTreeNode::File(size));
                } else {
                    let entry = children
                        .entry(name.clone())
                        .or_insert_with(|| FileTreeNode::Directory(BTreeMap::new()));
                    insert_path(entry, rest, size);
                }
            }
        }

        for file_path in files {
            let relative_path = file_path.strip_prefix(repo_path).unwrap_or(file_path);
            let components: Vec<String> = relative_path
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect();
            if components.is_empty() {
                continue;
            }
            let size = file_path.metadata().map(|m| m.len()).unwrap_or(0);
            insert_path(&mut root, &components, size);
        }

        // Helper function to print the tree structure
        fn print_structure(node: &FileTreeNode, indent: usize, output: &mut String) {
            match node {
                FileTreeNode::Directory(children) => {
                    for (name, child) in children {
                        let prefix = "  ".repeat(indent);
                        match child {
                            FileTreeNode::Directory(_) => {
                                output.push_str(&format!("{prefix}{name}/\n"));
                                print_structure(child, indent + 1, output);
                            }
                            FileTreeNode::File(size) => {
                                let size_str = if *size < 1024 {
                                    format!("({size} bytes)")
                                } else {
                                    format!("({}KB)", size / 1024)
                                };
                                output.push_str(&format!("{prefix}{name}{size_str}\n"));
                            }
                        }
                    }
                }
                FileTreeNode::File(_) => {
                    // Nothing to print at this level
                }
            }
        }

        print_structure(&root, 0, &mut output);
        output.push('\n');

        // --- File Contents ---
        output.push_str("FILE CONTENTS:\n");
        output.push_str(&"=".repeat(80));
        output.push('\n');

        for file_path in files {
            // When creating the file header, we need to make the path relative to the original source path,
            // not the single `repo_path` which might be the CWD.
            let display_path = file_path.strip_prefix(repo_path).unwrap_or(file_path);
            output.push_str(&format!("\nFILE: {}\n", display_path.display()));
            output.push_str(&"-".repeat(80));
            output.push('\n');

            match fs::read_to_string(file_path) {
                Ok(content) => {
                    for (i, line) in content.lines().enumerate() {
                        output.push_str(&format!("{:4}: {}\n", i + 1, line));
                    }
                }
                Err(_) => {
                    output.push_str("[Non-UTF-8 file or read error - skipped]\n");
                }
            }

            output.push_str(&"\n".repeat(2));
            output.push_str(&"-".repeat(80));
            output.push('\n');
        }

        output
    }

    /// The main conversion logic.
    pub fn convert_repository(
        &self,
        repo_sources: &[String],
        output_file: Option<String>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let mut all_files = Vec::new();
        let mut source_identifiers = Vec::new();

        for repo_source in repo_sources {
            let (source_path, _temp_dir) = if repo_source.starts_with("http") {
                let temp_dir = tempdir()?;
                let path = temp_dir.path().to_path_buf();
                self.clone_repository(repo_source, &path)?;
                (path, Some(temp_dir))
            } else {
                let path = PathBuf::from(repo_source);
                (path.canonicalize().unwrap_or(path), None)
            };

            println!("Collecting files from {repo_source}...");
            let files = self.collect_files(&source_path);
            println!("Found {} text files in {}.", files.len(), repo_source);
            all_files.extend(files);
            source_identifiers.push(repo_source.to_string());
        }

        let base_repo_path = if repo_sources.len() == 1 && Path::new(&repo_sources[0]).is_dir() {
            PathBuf::from(&repo_sources[0]).canonicalize()?
        } else {
            std::env::current_dir()?
        };

        println!("Generating LLM-friendly text...");
        let output_text = self.generate_llm_friendly_text(
            &base_repo_path,
            &all_files,
            &source_identifiers.join(", "),
        );

        let output_path = match output_file {
            Some(path) => PathBuf::from(path),
            None => {
                let repo_name = Path::new(&source_identifiers[0])
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("repo");
                PathBuf::from(format!("{repo_name}_llm_friendly.txt"))
            }
        };

        let mut file = File::create(&output_path)?;
        file.write_all(output_text.as_bytes())?;

        Ok(output_path.to_string_lossy().to_string())
    }
}

pub async fn run(args: PromptCMD) -> Result<()> {
    // Load configuration from file
    let config = match find_and_load_config() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Warning: Could not load or parse shelf.toml: {e}. Using defaults.");
            Config::default()
        }
    };

    let converter = RepoConverter::new(args.max_size, config);
    match converter.convert_repository(&args.repo_sources, args.output_file) {
        Ok(output_path) => {
            println!("\nConversion completed successfully!");
            println!("Output written to: {output_path}",);
            Ok(())
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use std::io::{self, Write};

    /// Helper function to create a temporary test directory with a predefined structure.
    fn setup_test_directory() -> io::Result<TempDir> {
        let dir = tempdir()?;
        let path = dir.path();

        // Root files
        fs::write(path.join("README.md"), "This is a test repo.")?;
        fs::write(
            path.join("main.rs"),
            "fn main() {\n    println!(\"Hello\");\n}",
        )?;
        fs::write(path.join(".gitignore"), "target/\n.DS_Store")?;

        // Skipped binary file
        fs::write(path.join("logo.png"), &[0x89, 0x50, 0x4E, 0x47])?;
        // Skipped large file
        let mut large_file = File::create(path.join("large_file.log"))?;
        let large_content = vec![0; 2 * 1024 * 1024]; // 2MB
        large_file.write_all(&large_content)?;

        // Skipped directories
        fs::create_dir(path.join("target"))?;
        fs::write(path.join("target").join("app.exe"), "binary")?;
        fs::create_dir(path.join(".vscode"))?;
        fs::write(path.join(".vscode").join("settings.json"), "{}")?;
        fs::create_dir(path.join("node_modules"))?;
        fs::write(path.join("node_modules").join("lib.js"), "var x = 1;")?;

        // Nested source directory
        fs::create_dir(path.join("src"))?;
        fs::write(path.join("src").join("lib.rs"), "pub fn run() {}")?;
        fs::write(path.join("src").join("module.py"), "# A python module")?;

        Ok(dir)
    }

    #[test]
    fn test_should_skip_directory() {
        let converter = RepoConverter::new(1024, Config::default());
        assert!(converter.should_skip_directory(Path::new("/project/.git")));
        assert!(converter.should_skip_directory(Path::new("/project/node_modules")));
        assert!(converter.should_skip_directory(Path::new("/project/target")));
        assert!(converter.should_skip_directory(Path::new("/project/build/")));
        assert!(!converter.should_skip_directory(Path::new("/project/src")));
    }

    #[test]
    fn test_should_skip_file() {
        let dir = setup_test_directory().unwrap();
        let converter = RepoConverter::new(1024 * 1024, Config::default()); // 1MB max size

        // Test skipping by name
        assert!(converter.should_skip_file(&dir.path().join(".gitignore")));
        assert!(!converter.should_skip_file(&dir.path().join("README.md")));

        // Test skipping by size
        assert!(converter.should_skip_file(&dir.path().join("large_file.log")));
    }

    #[test]
    fn test_is_text_file() {
        let dir = setup_test_directory().unwrap();
        let converter = RepoConverter::new(1024 * 1024, Config::default());

        assert!(converter.is_text_file(&dir.path().join("README.md")));
        assert!(converter.is_text_file(&dir.path().join("main.rs")));
        assert!(!converter.is_text_file(&dir.path().join("logo.png")));
    }

    #[test]
    fn test_collect_files() {
        let dir = setup_test_directory().unwrap();
        let converter = RepoConverter::new(1024 * 1024, Config::default());
        let files = converter.collect_files(dir.path());

        let expected_files: HashSet<PathBuf> = [
            dir.path().join("main.rs"),
            dir.path().join("README.md"),
            dir.path().join("src/lib.rs"),
            dir.path().join("src/module.py"),
        ]
        .iter()
        .cloned()
        .collect();

        let collected_files: HashSet<PathBuf> = files.into_iter().collect();

        assert_eq!(collected_files.len(), 4);
        assert_eq!(collected_files, expected_files);
    }

    #[test]
    fn test_generate_llm_friendly_text() {
        let dir = setup_test_directory().unwrap();
        let converter = RepoConverter::new(1024 * 1024, Config::default());
        let files = converter.collect_files(dir.path());
        let output = converter.generate_llm_friendly_text(dir.path(), &files, "test_repo");

        // Check header
        assert!(output.contains("REPOSITORY: test_repo"));

        // Check structure view
        assert!(output.contains("REPOSITORY STRUCTURE:"));
        assert!(output.contains("main.rs"));
        assert!(output.contains("src/"));
        assert!(output.contains("  lib.rs"));

        // Check file contents section
        assert!(output.contains("FILE CONTENTS:"));
        assert!(output.contains("FILE: main.rs"));
        assert!(output.contains("   1: fn main() {")); // Check line numbering
        assert!(output.contains("   2:     println!(\"Hello\");"));
        assert!(output.contains("FILE: src/lib.rs"));
        assert!(output.contains("   1: pub fn run() {}"));
    }

    #[test]
    fn test_full_conversion_process_local() {
        let dir = setup_test_directory().unwrap();
        let converter = RepoConverter::new(1024 * 1024, Config::default());
        let output_file_path = dir.path().join("output.txt");

        converter
            .convert_repository(
                &[dir.path().to_str().unwrap().to_string()],
                Some(output_file_path.to_str().unwrap().to_string()),
            )
            .unwrap();

        let output_content = fs::read_to_string(output_file_path).unwrap();

        // Check that skipped files/dirs are not in the output
        assert!(!output_content.contains("FILE: .gitignore"));
        assert!(!output_content.contains("FILE: logo.png"));
        assert!(!output_content.contains("FILE: large_file.log"));
        assert!(!output_content.contains("target/"));
        assert!(!output_content.contains("node_modules/"));

        // Check that included files are present
        assert!(output_content.contains("FILE: main.rs"));
        assert!(output_content.contains("fn main() {"));
        assert!(output_content.contains("FILE: README.md"));
        assert!(output_content.contains("This is a test repo."));
        assert!(output_content.contains("FILE: src/lib.rs"));
        assert!(output_content.contains("pub fn run() {}"));
    }
}
