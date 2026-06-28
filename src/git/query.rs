//! Read-only git operations.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{RepError, Result};

/// Run a git command in `root` and return trimmed stdout on success.
fn git_output(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| RepError::Git(format!("failed to execute git: {e}")))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(RepError::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}

/// Run a git command in `root` and report whether it succeeded.
fn git_check(root: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Discover the repository root from the current working directory.
pub fn discover_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(&cwd)
        .output()
        .map_err(|e| RepError::Git(format!("failed to execute git: {e}")))?;
    if output.status.success() {
        let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(PathBuf::from(root))
    } else {
        Err(RepError::NotAGitRepository)
    }
}

/// Get the current `HEAD` commit hash.
pub fn head(root: &Path) -> Result<String> {
    git_output(root, &["rev-parse", "HEAD"])
}

/// List all tracked files as repo-root-relative paths.
pub fn tracked_files(root: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .output()
        .map_err(|e| RepError::Git(format!("failed to execute git: {e}")))?;
    if !output.status.success() {
        return Err(RepError::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let files = output
        .stdout
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    Ok(files)
}

/// Check whether the tracked tree is clean (no staged or unstaged changes).
pub fn tracked_tree_clean(root: &Path) -> bool {
    git_check(root, &["diff", "--quiet"]) && git_check(root, &["diff", "--cached", "--quiet"])
}

/// Return the set of tracked file paths.
pub fn tracked_set(root: &Path) -> Result<std::collections::HashSet<String>> {
    Ok(tracked_files(root)?.into_iter().collect())
}
