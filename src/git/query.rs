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

/// Renames staged in the index relative to HEAD, as `(from, to)` pairs of
/// repo-root-relative paths.
///
/// Rename detection needs both sides tracked, so callers must stage their
/// renames (e.g. via `git mv`) first — a worktree-only rename is just a
/// deletion plus an untracked file and can never be paired.
pub fn staged_renames(root: &Path) -> Result<Vec<(String, String)>> {
    let output = Command::new("git")
        .args([
            "diff",
            "--cached",
            "--find-renames",
            "--name-status",
            "--diff-filter=R",
            "-z",
        ])
        .current_dir(root)
        .output()
        .map_err(|e| RepError::Git(format!("failed to execute git: {e}")))?;
    if !output.status.success() {
        return Err(RepError::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    parse_rename_records(&output.stdout)
}

/// Parse `--name-status -z` rename records: NUL-separated
/// `R<score> FROM TO` triples.
fn parse_rename_records(raw: &[u8]) -> Result<Vec<(String, String)>> {
    let mut fields = raw.split(|&b| b == 0).filter(|s| !s.is_empty());
    let mut renames = Vec::new();
    while let Some(status) = fields.next() {
        let status = String::from_utf8_lossy(status);
        if !status.starts_with('R') {
            return Err(RepError::Git(format!(
                "unexpected record status '{status}' in rename-filtered diff"
            )));
        }
        let from = fields.next().ok_or_else(|| {
            RepError::Git("truncated rename record: missing FROM path".to_string())
        })?;
        let to = fields
            .next()
            .ok_or_else(|| RepError::Git("truncated rename record: missing TO path".to_string()))?;
        renames.push((
            String::from_utf8_lossy(from).into_owned(),
            String::from_utf8_lossy(to).into_owned(),
        ));
    }
    Ok(renames)
}

/// Check whether the tracked tree is clean (no staged or unstaged changes).
pub fn tracked_tree_clean(root: &Path) -> bool {
    git_check(root, &["diff", "--quiet"]) && git_check(root, &["diff", "--cached", "--quiet"])
}

/// Return the set of tracked file paths.
pub fn tracked_set(root: &Path) -> Result<std::collections::HashSet<String>> {
    Ok(tracked_files(root)?.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_records_parse_score_from_to_triples() {
        let raw = b"R100\0src/oldname.ts\0src/newname.ts\0R087\0a/x.ts\0b/y.ts\0";
        let renames = parse_rename_records(raw).unwrap();
        assert_eq!(
            renames,
            vec![
                ("src/oldname.ts".to_string(), "src/newname.ts".to_string()),
                ("a/x.ts".to_string(), "b/y.ts".to_string()),
            ]
        );
    }

    #[test]
    fn rename_records_empty_input_is_empty() {
        assert!(parse_rename_records(b"").unwrap().is_empty());
    }

    #[test]
    fn rename_records_truncated_is_error() {
        assert!(parse_rename_records(b"R100\0only-from\0").is_err());
    }
}
