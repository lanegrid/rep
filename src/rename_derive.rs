//! Derive literal mappings from git-detected file renames.
//!
//! Only a rename that keeps its directory and extension but changes the file
//! stem yields an unambiguous token mapping (`old_stem=new_stem`). Every other
//! rename is reported as underivable — with a reason — instead of being
//! silently dropped, so the caller can add explicit `--map` entries. (A rename
//! with the same directory, extension, and stem would be the same path, so
//! those two reasons cover everything git can report.)

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{RepError, Result};
use crate::text::Mapping;

/// The rename moved the file to a different directory (e.g. a file-to-dir
/// move like `scenes/x.ts -> scenes/x/scene.ts`), so no single token rename
/// describes it.
pub const REASON_DIRECTORY_CHANGED: &str = "directory_changed";
/// The rename changed the extension, so the stem diff may not be a token
/// rename at all.
pub const REASON_EXTENSION_CHANGED: &str = "extension_changed";

/// A staged rename that no literal mapping can be derived from.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Underivable {
    pub from: String,
    pub to: String,
    pub reason: String,
}

/// Derive `old_stem=new_stem` mappings from `(from, to)` rename pairs.
///
/// Identical derivations from renames in different directories collapse into
/// one mapping; two renames deriving the same FROM stem with *different* TO
/// stems are a hard error (the mapping set would be ambiguous), reported with
/// both paths so the caller can pass explicit `--map` entries.
pub fn derive(renames: &[(String, String)]) -> Result<(Vec<Mapping>, Vec<Underivable>)> {
    let mut mappings: Vec<Mapping> = Vec::new();
    // FROM stem -> (TO stem, the rename's from-path) for dedup and conflicts.
    let mut seen: HashMap<String, (String, String)> = HashMap::new();
    let mut underivable = Vec::new();

    for (from, to) in renames {
        let (from_p, to_p) = (Path::new(from), Path::new(to));
        if from_p.parent() != to_p.parent() {
            underivable.push(Underivable {
                from: from.clone(),
                to: to.clone(),
                reason: REASON_DIRECTORY_CHANGED.to_string(),
            });
            continue;
        }
        if from_p.extension() != to_p.extension() {
            underivable.push(Underivable {
                from: from.clone(),
                to: to.clone(),
                reason: REASON_EXTENSION_CHANGED.to_string(),
            });
            continue;
        }
        let from_stem = from_p.file_stem().unwrap_or_default().to_string_lossy();
        let to_stem = to_p.file_stem().unwrap_or_default().to_string_lossy();
        match seen.get(from_stem.as_ref()) {
            Some((prev_to, _)) if *prev_to == to_stem => {} // same derivation again
            Some((_, prev_from)) => {
                return Err(RepError::InvalidArguments(format!(
                    "conflicting staged renames both derive FROM '{from_stem}': \
                     '{prev_from}' and '{from}'; pass explicit --map entries instead"
                )));
            }
            None => {
                seen.insert(from_stem.to_string(), (to_stem.to_string(), from.clone()));
                mappings.push(Mapping {
                    from: from_stem.to_string(),
                    to: to_stem.to_string(),
                });
            }
        }
    }

    Ok((mappings, underivable))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(from: &str, to: &str) -> (String, String) {
        (from.to_string(), to.to_string())
    }

    #[test]
    fn same_dir_stem_change_derives_mapping() {
        let (maps, und) = derive(&[r("src/01_lofi.ts", "src/lofi.ts")]).unwrap();
        assert_eq!(maps.len(), 1);
        assert_eq!(maps[0].from, "01_lofi");
        assert_eq!(maps[0].to, "lofi");
        assert!(und.is_empty());
    }

    #[test]
    fn directory_change_is_underivable() {
        let (maps, und) = derive(&[r("scenes/38_x.ts", "scenes/x/scene.ts")]).unwrap();
        assert!(maps.is_empty());
        assert_eq!(und.len(), 1);
        assert_eq!(und[0].reason, REASON_DIRECTORY_CHANGED);
    }

    #[test]
    fn extension_change_is_underivable() {
        let (maps, und) = derive(&[r("docs/notes.txt", "docs/notes.md")]).unwrap();
        assert!(maps.is_empty());
        assert_eq!(und[0].reason, REASON_EXTENSION_CHANGED);
    }

    #[test]
    fn identical_derivations_across_dirs_dedupe() {
        let (maps, _) = derive(&[r("a/x.ts", "a/y.ts"), r("b/x.ts", "b/y.ts")]).unwrap();
        assert_eq!(maps.len(), 1);
    }

    #[test]
    fn conflicting_derivations_error_with_both_paths() {
        let err = derive(&[r("a/x.ts", "a/y.ts"), r("b/x.ts", "b/z.ts")]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("a/x.ts"), "message: {msg}");
        assert!(msg.contains("b/x.ts"), "message: {msg}");
    }

    #[test]
    fn only_final_extension_defines_the_stem() {
        // Rust Path semantics: only the last `.ext` is the extension, so
        // `38_x.test` is the stem of `38_x.test.ts`.
        let (maps, _) = derive(&[r("a/38_x.test.ts", "a/x.test.ts")]).unwrap();
        assert_eq!(maps[0].from, "38_x.test");
        assert_eq!(maps[0].to, "x.test");
    }
}
