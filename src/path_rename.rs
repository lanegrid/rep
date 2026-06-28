//! File-level path rename planning and conflict detection.
//!
//! The minimal version operates on tracked *file* paths (not directories) and
//! never auto-resolves conflicts: any collision fails the plan.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{RepError, Result};
use crate::scope;
use crate::text::Mapping;

/// A single planned rename `from -> to`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rename {
    pub from: String,
    pub to: String,
    pub sha256_before: String,
}

/// Compute the set of renames implied by applying `maps` to `paths`.
///
/// `tracked` is the full set of tracked paths and `root` is used to probe the
/// filesystem for untracked collisions. Returns an error on the first
/// conflict detected.
pub fn plan_renames(
    root: &Path,
    paths: &[String],
    maps: &[Mapping],
    tracked: &HashSet<String>,
    hash_of: impl Fn(&str) -> Result<String>,
) -> Result<Vec<Rename>> {
    let mut renames = Vec::new();
    let mut targets: HashMap<String, String> = HashMap::new();

    for from in paths {
        let to = scope::rewrite_path(from, maps);
        if &to == from {
            continue;
        }

        // Case-only renames are unsafe on case-insensitive filesystems; the
        // minimal version rejects them rather than guessing the filesystem.
        if from.eq_ignore_ascii_case(&to) {
            return Err(RepError::PathConflict(format!(
                "case-only rename '{from}' -> '{to}' is unsafe (case_only_rename_unsafe)"
            )));
        }

        // Two sources rewriting to the same target.
        if let Some(other) = targets.get(&to) {
            return Err(RepError::PathConflict(format!(
                "'{other}' and '{from}' both rename to '{to}'"
            )));
        }
        // Target collides with an existing tracked file (or another rename
        // source, which is a subset of the tracked set).
        if tracked.contains(&to) {
            return Err(RepError::PathConflict(format!(
                "rename target '{to}' already exists as a tracked file"
            )));
        }
        // Target collides with an untracked file on disk.
        if root.join(&to).exists() {
            return Err(RepError::PathConflict(format!(
                "rename target '{to}' already exists as an untracked file"
            )));
        }

        targets.insert(to.clone(), from.clone());
        renames.push(Rename {
            from: from.clone(),
            to,
            sha256_before: hash_of(from)?,
        });
    }

    Ok(renames)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::Mapping;
    use std::collections::HashSet;

    fn maps() -> Vec<Mapping> {
        vec![Mapping {
            from: "oldname".to_string(),
            to: "newname".to_string(),
        }]
    }

    #[test]
    fn renames_only_changed_paths() {
        let paths = vec!["src/oldname.ts".to_string(), "src/keep.ts".to_string()];
        let tracked: HashSet<String> = paths.iter().cloned().collect();
        let root = std::env::temp_dir().join("rep-nonexistent-xyz");
        let renames =
            plan_renames(&root, &paths, &maps(), &tracked, |_| Ok("hash".to_string())).unwrap();
        assert_eq!(renames.len(), 1);
        assert_eq!(renames[0].from, "src/oldname.ts");
        assert_eq!(renames[0].to, "src/newname.ts");
    }

    #[test]
    fn duplicate_target_is_conflict() {
        let paths = vec!["a/oldname.ts".to_string(), "a/newname.ts".to_string()];
        let tracked: HashSet<String> = paths.iter().cloned().collect();
        let root = std::env::temp_dir().join("rep-nonexistent-xyz");
        // oldname.ts -> newname.ts collides with the already-tracked newname.ts
        let err = plan_renames(&root, &paths, &maps(), &tracked, |_| Ok("h".to_string()));
        assert!(err.is_err());
    }
}
