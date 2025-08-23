use anyhow::{self, Result};
use clap::Parser;
use git2::Repository;
use glob::Pattern;
use handlebars::Handlebars;
use indicatif::{ProgressBar, ProgressStyle};
use rig::{client::builder::DynClientBuilder, completion::Prompt};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use termimad::{self, MadSkin};
use tracing::debug;
use walkdir::WalkDir;

use crate::config::{Config, find_and_load_config};

#[derive(Parser, Debug, Clone)]
#[command(
    author,
    version,
    about = "Converts a GitHub repository or local directory into an LLM-friendly text file.",
    long_about = "This command processes specified repositories or local directories, \
                  filters files based on size and user-defined patterns, and generates \
                  a structured text output suitable for Large Language Models. \
                  It includes a repository structure overview and file contents, \
                  and allows for custom templating."
)]
pub struct PromptCMD {
    /// Specify one or more GitHub repository URLs (e.g., 'https://github.com/user/repo')
    /// or local directory paths (e.g., './my_project', '../another_repo').
    ///
    /// If a local directory is provided, it will be scanned directly.
    /// GitHub URLs will be cloned to a temporary directory.
    #[arg(num_args = 1.., help_heading = "Input Options")]
    repo_sources: Vec<String>,

    /// Path to the output file where the LLM-friendly text will be written.
    ///
    /// If not specified, a default filename like 'repo_llm_friendly.txt' will be used
    /// in the current working directory.
    /// Example: --output-file my_project_context.txt
    #[arg(short, long, help_heading = "Output Options")]
    output_file: Option<String>,

    /// Maximum file size in bytes to include in the output. Files exceeding this size will be skipped.
    ///
    /// Default is 1MB (1024 * 1024 bytes). This helps prevent including excessively large
    /// files that might exhaust token limits or contain irrelevant data.
    /// Example: --max-size 524288 (for 512KB)
    #[arg(long, default_value_t = 1024 * 1024, help_heading = "Filtering Options")]
    max_size: u64,

    /// Glob patterns for files to explicitly include. Only files matching these patterns will be processed.
    ///
    /// This argument can be specified multiple times. Patterns are relative to the repository root.
    /// Example: --include "src/**/*.rs" --include "config/*.toml"
    #[arg(long, value_name = "GLOB", value_delimiter = ',', num_args = 1.., help_heading = "Filtering Options")]
    include: Option<Vec<String>>,

    /// Glob patterns for files to explicitly exclude. Files matching these patterns will be ignored.
    ///
    /// This argument can be specified multiple times. Patterns are relative to the repository root.
    /// Exclusion patterns take precedence over inclusion patterns.
    /// Example: --exclude "**/.gitkeep" --exclude "tests/**"
    #[arg(long, value_name = "GLOB", value_delimiter = ',', num_args = 1.., help_heading = "Filtering Options")]
    exclude: Option<Vec<String>>,

    /// Disable the printing of line numbers alongside the code content in the generated output file.
    ///
    /// By default, line numbers are included to provide better context for LLMs when referring to specific lines.
    #[arg(long, help_heading = "Output Options")]
    no_line_numbers: bool,

    /// Path to a Handlebars template file used to wrap the generated repository context.
    ///
    /// The template can use the `{{REPOSITORY_CONTEXT}}` placeholder where the generated
    /// file content should be inserted. This allows for custom pre-prompts or post-prompts.
    /// Example: --template "path/to/my_template.hbs"
    ///
    /// A simple template file `my_template.hbs` might look like:
    /// ```
    /// You are an expert programmer. Here is a codebase:
    /// {{REPOSITORY_CONTEXT}}
    ///
    /// Based on the code above, please explain the main architecture.
    /// ```
    #[arg(short, long, value_name = "PATH", help_heading = "Output Options")]
    template: Option<PathBuf>,

    /// AI model provider to use for generation.
    #[arg(long, default_value = "gemini")]
    provider: String,

    /// Specific model to use for the prompt.
    #[arg(long, default_value = "gemini-2.5-flash-lite")]
    model: String,

    /// System prompt to guide the AI's behavior.
    #[arg(long)]
    preamble: Option<String>,

    /// Execute the prompt with the AI model instead of saving to a file.
    #[arg(long, short)]
    execute: bool,
}

/// A converter for GitHub repositories or local directories to an LLM-friendly text file.
struct RepoConverter {
    max_file_size: u64,
    config: Config,
    args: PromptCMD,
}

// An enum to represent our file system tree structure.
// This solves the "cyclic type" error by defining a recursive structure correctly.
enum FileTreeNode {
    File(u64 /* size */),
    Directory(BTreeMap<String, FileTreeNode>),
}

impl RepoConverter {
    fn new(args: PromptCMD, config: Config) -> Self {
        Self {
            max_file_size: args.max_size,
            config,
            args,
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
            && skip_files
                .iter()
                .any(|p| glob::Pattern::new(p).is_ok_and(|pat| pat.matches(file_name)))
        {
            debug!(
                "Skipping file due to skip_files pattern: {}",
                file_path.display()
            );
            return true;
        }

        if let Ok(metadata) = fs::metadata(file_path)
            && metadata.len() > self.max_file_size
        {
            debug!(
                "Skipping file due to size ({} > {}): {}",
                metadata.len(),
                self.max_file_size,
                file_path.display()
            );
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

        if let Some(dir_name) = dir_path.file_name().and_then(|n| n.to_str())
            && skip_directories.contains(dir_name)
        {
            debug!(
                "Skipping directory due to skip_directories: {}",
                dir_path.display()
            );
            return true;
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
        // 1. Initial collection with default skips
        let initial_files: Vec<_> = WalkDir::new(repo_path)
            .into_iter()
            .filter_entry(|e| !self.should_skip_directory(e.path()))
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_type().is_file()
                    && !self.should_skip_file(e.path())
                    && self.is_text_file(e.path())
            })
            .map(|e| e.into_path())
            .collect();

        // 2. Apply --exclude patterns
        let mut excluded_files = initial_files;
        if let Some(exclude_patterns) = &self.args.exclude {
            let globs: Vec<_> = exclude_patterns
                .iter()
                .filter_map(|p| Pattern::new(p).ok())
                .collect();
            excluded_files.retain(|path| {
                let relative_path = path.strip_prefix(repo_path).unwrap_or(path);
                let is_excluded = globs.iter().any(|glob| glob.matches_path(relative_path));
                if is_excluded {
                    debug!("Excluding file due to --exclude glob: {}", path.display());
                }
                !is_excluded
            });
        }

        // 3. Apply --include patterns if provided
        let mut final_files = if let Some(include_patterns) = &self.args.include {
            let globs: Vec<_> = include_patterns
                .iter()
                .filter_map(|p| Pattern::new(p).ok())
                .collect();
            excluded_files
                .into_iter()
                .filter(|path| {
                    let relative_path = path.strip_prefix(repo_path).unwrap_or(path);
                    let is_included = globs.iter().any(|glob| glob.matches_path(relative_path));
                    if !is_included {
                        debug!(
                            "Excluding file due to missing --include glob: {}",
                            path.display()
                        );
                    }
                    is_included
                })
                .collect()
        } else {
            excluded_files
        };

        // Sort for consistent output
        final_files.sort();
        final_files
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
        fn print_structure(node: &FileTreeNode, prefix: &str, output: &mut String) {
            if let FileTreeNode::Directory(children) = node {
                let mut entries = children.iter().peekable();
                while let Some((name, child)) = entries.next() {
                    let connector = if entries.peek().is_some() {
                        "├── "
                    } else {
                        "└── "
                    };

                    match child {
                        FileTreeNode::Directory(_) => {
                            output.push_str(&format!("{prefix}{connector}{name}/\n"));
                            let new_prefix = format!(
                                "{}{}",
                                prefix,
                                if entries.peek().is_some() {
                                    "│   "
                                } else {
                                    "    "
                                }
                            );
                            print_structure(child, &new_prefix, output);
                        }
                        FileTreeNode::File(size) => {
                            let size_str = if *size < 1024 {
                                format!("({size} bytes)")
                            } else {
                                format!("({}KB)", size / 1024)
                            };
                            output.push_str(&format!("{prefix}{connector}{name} {size_str}\n"));
                        }
                    }
                }
            }
        }

        print_structure(&root, "", &mut output);
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
                    if self.args.no_line_numbers {
                        output.push_str(&content);
                        output.push('\n'); // Ensure trailing newline if missing
                    } else {
                        for (i, line) in content.lines().enumerate() {
                            output.push_str(&format!("{:4}: {}\n", i + 1, line));
                        }
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

    /// Applies a template to the generated context if one is provided.
    fn apply_template(&self, context: String) -> Result<String> {
        if let Some(template_path) = &self.args.template {
            debug!("Applying template from: {}", template_path.display());
            let template_content = fs::read_to_string(template_path).map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to read template file '{}': {}",
                        template_path.display(),
                        e
                    ),
                )
            })?;

            let mut handlebars = Handlebars::new();
            handlebars.register_escape_fn(handlebars::no_escape); // To prevent HTML escaping

            let data = serde_json::json!({
                "REPOSITORY_CONTEXT": context
            });

            let rendered = handlebars.render_template(&template_content, &data)?;
            Ok(rendered)
        } else {
            Ok(context)
        }
    }

    /// Generates the context for the prompt.
    fn generate_context_from_sources(
        &self,
        repo_sources: &[String],
        _output_file: Option<String>, // Mark as unused for now
    ) -> Result<String, Box<dyn std::error::Error>> {
        let mut all_files = Vec::new();
        let mut source_identifiers = Vec::new();

        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
                .template("{spinner:.green} Processing: {msg}")?,
        );

        for repo_source in repo_sources {
            spinner.set_message(repo_source.clone());
            spinner.tick();

            let (source_path, _temp_dir) = if repo_source.starts_with("http") {
                let temp_dir = tempdir()?;
                let path = temp_dir.path().to_path_buf();
                self.clone_repository(repo_source, &path)?;
                (path, Some(temp_dir))
            } else {
                let path = PathBuf::from(repo_source);
                (path, None)
            };

            debug!("Collecting files from source: {}", repo_source);
            let files = self.collect_files(&source_path);
            debug!("Found {} text files in {}.", files.len(), repo_source);
            all_files.extend(files);
            source_identifiers.push(repo_source.to_string());
        }

        let base_repo_path = if repo_sources.len() == 1 && Path::new(&repo_sources[0]).is_dir() {
            PathBuf::from(&repo_sources[0]).canonicalize()?
        } else {
            std::env::current_dir()?
        };

        spinner.finish_with_message(format!("Collected {} files total.", all_files.len()));

        println!("Generating LLM-friendly text...");
        Ok(self.generate_llm_friendly_text(
            &base_repo_path,
            &all_files,
            &source_identifiers.join(", "),
        ))
    }

    /// Saves the generated content to a file.
    fn save_to_file(
        &self,
        content: &str,
        output_file: Option<String>,
        source_identifiers: &[String],
    ) -> Result<String> {
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
        file.write_all(content.as_bytes())?;

        Ok(output_path.to_string_lossy().to_string())
    }
}

/// Sets up the AI client and executes the prompt, streaming the response to stdout.
async fn execute_with_ai(prompt: String, args: &PromptCMD) -> Result<()> {
    debug!(
        "Executing with AI provider: {}, model: {}",
        args.provider, args.model
    );

    let client = DynClientBuilder::new();
    let mut agent_builder = client.agent(&args.provider, &args.model)?;

    if let Some(preamble) = &args.preamble {
        agent_builder = agent_builder.preamble(preamble);
    }

    let agent = agent_builder.build();

    match agent.prompt(prompt).await {
        Ok(response) => {
            let skin = MadSkin::default();
            skin.print_text(&response);
            Ok(())
        }
        Err(e) => {
            // Catch rig-core errors by inspecting their string output for key phrases.
            let error_string = e.to_string();
            let suggestion = if error_string.contains("No API key found") {
                "Suggestion: Please set the required environment variable for your provider (e.g., 'GEMINI_API_KEY')."
            } else if error_string.contains("invalid API key") {
                "Suggestion: The provided API key is invalid or expired. Please check your credentials."
            } else if error_string.contains("404 Not Found")
                || error_string.contains("Model not found")
            {
                "Suggestion: The model name seems incorrect or is not available. Please check the model name and try again."
            } else {
                "Suggestion: Check your network connection, API credentials, and the model name."
            };

            let full_error_message = format!("AI Execution Error: {e}\n\n{suggestion}");

            Err(anyhow::anyhow!(full_error_message))
        }
    }
}

pub async fn run(args: PromptCMD) -> Result<()> {
    let config = find_and_load_config()?;

    let converter = RepoConverter::new(args.clone(), config);

    // This logic is now shared, but the final output is handled differently.
    let _final_prompt = if args.execute {
        // AI Execution Workflow
        let spinner = ProgressBar::new_spinner().with_message("AI is thinking...");
        spinner.enable_steady_tick(std::time::Duration::from_millis(100));
        let context = converter
            .generate_context_from_sources(&args.repo_sources, args.output_file.clone())
            .unwrap();
        let final_prompt = converter.apply_template(context)?;
        let result = execute_with_ai(final_prompt.clone(), &args).await;
        spinner.finish_and_clear();
        result?; // Propagate error if AI execution fails
        final_prompt // Return the prompt if execution was successful (though not used here)
    } else {
        // Save to File Workflow
        let context = converter
            .generate_context_from_sources(&args.repo_sources, args.output_file.clone())
            .unwrap();
        let final_prompt = converter.apply_template(context)?;
        let output_path_str =
            converter.save_to_file(&final_prompt, args.output_file.clone(), &args.repo_sources)?;
        println!("\nConversion completed successfully!");
        println!("Output written to: {output_path_str}");
        final_prompt // Return the generated prompt for the sake of the return type
    };

    Ok(())
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
        let args = PromptCMD {
            repo_sources: vec![],
            output_file: None,
            max_size: 1024,
            include: None,
            exclude: None,
            no_line_numbers: false,
            template: None,
            provider: "gemini".to_string(),
            model: "test".to_string(),
            preamble: None,
            execute: false,
        };
        let converter = RepoConverter::new(args, Config::default());
        assert!(converter.should_skip_directory(Path::new("/project/.git")));
        assert!(converter.should_skip_directory(Path::new("/project/node_modules")));
        assert!(converter.should_skip_directory(Path::new("/project/target")));
        assert!(converter.should_skip_directory(Path::new("/project/build/")));
        assert!(!converter.should_skip_directory(Path::new("/project/src")));
    }

    #[test]
    fn test_should_skip_file() {
        let dir = setup_test_directory().unwrap();
        let args = PromptCMD {
            repo_sources: vec![],
            output_file: None,
            max_size: 1024 * 1024, // 1MB max size
            include: None,
            exclude: None,
            no_line_numbers: false,
            template: None,
            provider: "gemini".to_string(),
            model: "test".to_string(),
            preamble: None,
            execute: false,
        };
        let converter = RepoConverter::new(args, Config::default());

        // Test skipping by name
        assert!(converter.should_skip_file(&dir.path().join(".gitignore")));
        assert!(!converter.should_skip_file(&dir.path().join("README.md")));

        // Test skipping by size
        assert!(converter.should_skip_file(&dir.path().join("large_file.log")));
    }

    #[test]
    fn test_is_text_file() {
        let dir = setup_test_directory().unwrap();
        let args = PromptCMD {
            repo_sources: vec![],
            output_file: None,
            max_size: 1024 * 1024,
            include: None,
            exclude: None,
            no_line_numbers: false,
            template: None,
            provider: "gemini".to_string(),
            model: "test".to_string(),
            preamble: None,
            execute: false,
        };
        let converter = RepoConverter::new(args, Config::default());

        assert!(converter.is_text_file(&dir.path().join("README.md")));
        assert!(converter.is_text_file(&dir.path().join("main.rs")));
        assert!(!converter.is_text_file(&dir.path().join("logo.png")));
    }

    #[test]
    fn test_collect_files() {
        let dir = setup_test_directory().unwrap();
        let args = PromptCMD {
            repo_sources: vec![],
            output_file: None,
            max_size: 1024 * 1024,
            include: None,
            exclude: None,
            no_line_numbers: false,
            template: None,
            provider: "gemini".to_string(),
            model: "test".to_string(),
            preamble: None,
            execute: false,
        };
        let converter = RepoConverter::new(args, Config::default());
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
        let args = PromptCMD {
            repo_sources: vec![dir.path().to_str().unwrap().to_string()],
            output_file: None,
            max_size: 1024 * 1024,
            include: None,
            exclude: None,
            no_line_numbers: false,
            template: None,
            provider: "gemini".to_string(),
            model: "test".to_string(),
            preamble: None,
            execute: false,
        };
        let converter = RepoConverter::new(args, Config::default());
        let files = converter.collect_files(dir.path());
        let output = converter.generate_llm_friendly_text(dir.path(), &files, "test_repo");

        // Check header
        assert!(output.contains("REPOSITORY: test_repo"));

        // Check structure view
        assert!(output.contains("REPOSITORY STRUCTURE:"));
        assert!(output.contains("main.rs"));
        assert!(output.contains("└── src/"));
        assert!(output.contains("    ├── lib.rs"));

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
        let args = PromptCMD {
            repo_sources: vec![dir.path().to_str().unwrap().to_string()],
            output_file: None,
            max_size: 1024 * 1024,
            include: None,
            exclude: None,
            no_line_numbers: false,
            template: None,
            provider: "gemini".to_string(),
            model: "test".to_string(),
            preamble: None,
            execute: false,
        };
        let converter = RepoConverter::new(args, Config::default());
        let output_file_path = dir.path().join("output.txt");

        let repo_sources = vec![dir.path().to_str().unwrap().to_string()];

        // Manually replicate the new logic from the `run` function for the test
        let context = converter
            .generate_context_from_sources(
                &repo_sources,
                Some(output_file_path.to_str().unwrap().to_string()),
            )
            .unwrap();
        let final_prompt = converter.apply_template(context).unwrap();
        let result = converter.save_to_file(
            &final_prompt,
            Some(output_file_path.to_str().unwrap().to_string()),
            &repo_sources,
        );

        // Ensure the save succeeded
        result.unwrap();

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

    #[test]
    fn test_config_loading() {
        let temp_dir = tempdir().unwrap();
        let current_dir = temp_dir.path();

        // Preserve original environment to avoid leaking changes between tests
        let original_home = std::env::var_os("HOME");
        let original_cwd = std::env::current_dir().ok();

        // Temporarily override HOME to isolate the test from real user configs
        unsafe { std::env::set_var("HOME", current_dir) };

        // Test 1: No config file found
        std::env::set_current_dir(current_dir).unwrap();
        let config = find_and_load_config().unwrap();
        assert!(config.prompt.skip_directories.is_empty());

        // Test 2: shelf.toml is found
        let toml_content = r#"
    [prompt]
    skip_directories = ["docs/"]
    skip_files = ["*.lock"]
    "#;
        fs::write(current_dir.join("shelf.toml"), toml_content).unwrap();
        let config = find_and_load_config().unwrap();
        assert_eq!(config.prompt.skip_directories, vec!["docs/"]);
        assert_eq!(config.prompt.skip_files, vec!["*.lock"]);

        // Test 3: Finds config in parent directory
        let sub_dir = current_dir.join("subdir");
        fs::create_dir(&sub_dir).unwrap();
        std::env::set_current_dir(&sub_dir).unwrap();
        let config = find_and_load_config().unwrap();
        assert_eq!(config.prompt.skip_directories, vec!["docs/"]);

        // Restore original environment
        if let Some(home) = original_home {
            unsafe { std::env::set_var("HOME", home) };
        } else {
            unsafe { std::env::remove_var("HOME") };
        }
        if let Some(cwd) = original_cwd {
            std::env::set_current_dir(cwd).unwrap();
        }
    }

    #[test]
    fn test_glob_filtering() {
        let dir = setup_test_directory().unwrap();

        // Test --exclude
        let args_exclude = PromptCMD {
            repo_sources: vec![],
            output_file: None,
            max_size: 1024 * 1024,
            include: None,
            template: None,
            exclude: Some(vec!["**/*.rs".to_string()]),
            no_line_numbers: false,
            preamble: None,
            provider: "".to_string(),
            model: "".to_string(),
            execute: false,
        };
        let converter = RepoConverter::new(args_exclude.clone(), Config::default());
        let files = converter.collect_files(dir.path());
        assert_eq!(files.len(), 2); // README.md, src/module.py
        assert!(files.iter().all(|p| !p.to_str().unwrap().ends_with(".rs")));

        // Test --include
        let args_include = PromptCMD {
            include: Some(vec!["**/README.md".to_string()]),
            exclude: None,
            ..args_exclude.clone()
        };
        let converter = RepoConverter::new(args_include, Config::default());
        let files = converter.collect_files(dir.path());
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_name().unwrap(), "README.md");

        // Test combination: include and exclude
        let args_combo = PromptCMD {
            include: Some(vec!["src/**/*".to_string()]), // include everything in src
            exclude: Some(vec!["**/*.py".to_string()]),  // but exclude python files
            ..args_exclude.clone()
        };
        let converter = RepoConverter::new(args_combo, Config::default());
        let files = converter.collect_files(dir.path());
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_name().unwrap(), "lib.rs");
    }

    #[test]
    fn test_template_application() {
        let dir = tempdir().unwrap();
        let template_path = dir.path().join("template.hbs");
        fs::write(
            &template_path,
            "Analyze this code:\n```\n{{REPOSITORY_CONTEXT}}\n```",
        )
        .unwrap();

        let args = PromptCMD {
            repo_sources: vec![],
            output_file: None,
            max_size: 1024 * 1024,
            include: None,
            exclude: None,
            no_line_numbers: false,
            preamble: None,
            provider: "".to_string(),
            model: "".to_string(),
            execute: false,
            template: Some(template_path),
        };

        let converter = RepoConverter::new(args, Config::default());
        let context = "Hello, World!".to_string();
        let rendered = converter.apply_template(context).unwrap();

        assert!(rendered.starts_with("Analyze this code:\n```"));
        assert!(rendered.ends_with("\n```"));
        assert!(rendered.contains("Hello, World!"));
    }
}
