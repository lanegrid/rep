//! State-changing git operations.

use std::path::Path;
use std::process::Command;

use crate::error::{RepError, Result};

/// Move a tracked file with `git mv`, creating the target's parent directory
/// first if necessary.
pub fn mv(root: &Path, from: &str, to: &str) -> Result<()> {
    if let Some(parent) = Path::new(to).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(root.join(parent))?;
        }
    }
    let output = Command::new("git")
        .args(["mv", from, to])
        .current_dir(root)
        .output()
        .map_err(|e| RepError::Git(format!("failed to execute git: {e}")))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(RepError::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}
