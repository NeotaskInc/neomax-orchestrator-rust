use std::fs;
use std::path::{Path, PathBuf};

pub fn discover_repositories(root: &Path) -> Vec<PathBuf> {
    if root.join(".git").exists() {
        return vec![PathBuf::from(".")];
    }
    let mut repositories = fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().join(".git").exists())
        .map(|entry| PathBuf::from(entry.file_name()))
        .collect::<Vec<_>>();
    repositories.sort();
    if repositories.is_empty() {
        repositories.push(PathBuf::from("."));
    }
    repositories
}
