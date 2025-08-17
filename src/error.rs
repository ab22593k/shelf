use std::path::{self, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Shelfor {
    #[error("Home directory not found")]
    HomeDirectoryNotFound,
    #[error("Path not found: {0:?}")]
    PathNotFound(PathBuf),
    #[error("Path is outside work tree: {0:?}")]
    OutsideWorkTree(PathBuf),
    #[error("Invalid UTF-8 in path")]
    InvalidUtf8Path,
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Path strip error: {0}")]
    StripPrefix(#[from] path::StripPrefixError),
    #[error("Git executable is not installed")]
    GitNotInstalled,
}
