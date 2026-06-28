//! Scope resolution: which tracked files are in play, and why others were
//! skipped.
//!
//! The `.rep/` directory is *always* excluded and may not be re-included.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{RepError, Result};
use crate::globset;
use crate::schema;
use crate::{git, text};

/// User-provided scope options shared by scan / plan / residual.
#[derive(Clone, Debug, Default)]
pub struct ScopeOpts {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub tracked_only: bool,
}

/// Serialized scope description embedded in JSON output and plan artifacts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scope {
    pub tracked_only: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub default_exclude: Vec<String>,
}

impl Scope {
    pub fn from_opts(opts: &ScopeOpts) -> Self {
        Scope {
            // The minimal version always operates on tracked files only.
            tracked_only: true,
            include: opts.include.clone(),
            exclude: opts.exclude.clone(),
            default_exclude: vec![schema::DEFAULT_EXCLUDE.to_string()],
        }
    }
}

/// A skipped path together with a machine-readable reason.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Skip {
    pub path: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_rule: Option<String>,
}

/// Decoded content of a tracked file that participates in scanning.
pub struct TextFile {
    pub path: String,
    pub content: String,
}

/// The result of resolving scope against the tracked tree.
pub struct Gathered {
    /// Decoded UTF-8 text files (content scan / replacement candidates).
    pub files: Vec<TextFile>,
    /// All in-scope tracked paths, including binary ones (path scan / rename).
    pub all_paths: Vec<String>,
    /// Skipped paths with reasons.
    pub skipped: Vec<Skip>,
    pub binary_skipped: usize,
    pub utf8_decode_skipped: usize,
}

/// Reject any attempt to include or target `.rep/` (safety invariant).
pub fn reject_rep_dir(opts: &ScopeOpts) -> Result<()> {
    for g in opts.include.iter().chain(opts.exclude.iter()) {
        if g == schema::REP_DIR || g.starts_with(".rep/") || g.starts_with(".rep\\") {
            return Err(RepError::InvalidArguments(format!(
                "'{g}' targets the reserved {} directory, which is always excluded",
                schema::REP_DIR
            )));
        }
    }
    Ok(())
}

fn is_rep_internal(path: &str) -> bool {
    path == schema::REP_DIR
        || path.starts_with(".rep/")
        || globset::glob_match(schema::DEFAULT_EXCLUDE, path)
}

/// Read a file, classifying it as text, binary, or non-UTF-8.
enum Decoded {
    Text(String),
    Binary,
    NotUtf8,
}

fn read_file(path: &Path) -> Result<Decoded> {
    let bytes = std::fs::read(path)?;
    if bytes.contains(&0) {
        return Ok(Decoded::Binary);
    }
    match String::from_utf8(bytes) {
        Ok(s) => Ok(Decoded::Text(s)),
        Err(_) => Ok(Decoded::NotUtf8),
    }
}

/// Resolve scope against the repository's tracked tree.
pub fn gather(root: &Path, opts: &ScopeOpts) -> Result<Gathered> {
    let tracked = git::tracked_files(root)?;
    let mut files = Vec::new();
    let mut all_paths = Vec::new();
    let mut skipped = Vec::new();
    let mut binary_skipped = 0;
    let mut utf8_decode_skipped = 0;

    for path in tracked {
        if is_rep_internal(&path) {
            skipped.push(Skip {
                path,
                reason: "rep_internal".to_string(),
                matched_rule: None,
            });
            continue;
        }
        // When --include is given, a path must match at least one include glob.
        if !opts.include.is_empty() && !globset::any_match(&opts.include, &path) {
            continue;
        }
        if let Some(rule) = globset::first_match(&opts.exclude, &path) {
            skipped.push(Skip {
                path,
                reason: "excluded_by_glob".to_string(),
                matched_rule: Some(rule.to_string()),
            });
            continue;
        }

        all_paths.push(path.clone());

        let full = root.join(&path);
        // Do not follow symlinks; their target content is out of scope.
        let meta = std::fs::symlink_metadata(&full)?;
        if meta.file_type().is_symlink() {
            continue;
        }

        match read_file(&full)? {
            Decoded::Text(content) => files.push(TextFile { path, content }),
            Decoded::Binary => {
                binary_skipped += 1;
                skipped.push(Skip {
                    path,
                    reason: "binary_nul_detected".to_string(),
                    matched_rule: None,
                });
            }
            Decoded::NotUtf8 => {
                utf8_decode_skipped += 1;
                skipped.push(Skip {
                    path,
                    reason: "utf8_decode_failed".to_string(),
                    matched_rule: None,
                });
            }
        }
    }

    Ok(Gathered {
        files,
        all_paths,
        skipped,
        binary_skipped,
        utf8_decode_skipped,
    })
}

/// Apply mappings to a path string, returning the rewritten path.
pub fn rewrite_path(path: &str, maps: &[text::Mapping]) -> String {
    text::apply(path, maps).0
}

/// Hex-encoded SHA-256 of arbitrary bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Hex-encoded SHA-256 of a file on disk.
pub fn sha256_file(path: &Path) -> Result<String> {
    Ok(sha256_hex(&std::fs::read(path)?))
}
