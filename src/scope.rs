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
///
/// The `config_*` fields are populated from `rep.toml` by [`resolve`] and kept
/// separate from the CLI vectors so a skipped path can be attributed to the
/// flag or the config file that excluded it.
#[derive(Clone, Debug, Default)]
pub struct ScopeOpts {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub tracked_only: bool,
    /// Skip loading `rep.toml` for this run (`--no-config`).
    pub no_config: bool,
    pub config_path: Option<String>,
    pub config_include: Vec<String>,
    pub config_exclude: Vec<String>,
}

/// Serialized scope description embedded in JSON output and plan artifacts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scope {
    pub tracked_only: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub default_exclude: Vec<String>,
    /// The config file the `config_*` globs came from; absent when no
    /// `rep.toml` exists or `--no-config` was given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config_include: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config_exclude: Vec<String>,
}

impl Scope {
    pub fn from_opts(opts: &ScopeOpts) -> Self {
        Scope {
            // The minimal version always operates on tracked files only.
            tracked_only: true,
            include: opts.include.clone(),
            exclude: opts.exclude.clone(),
            default_exclude: vec![schema::DEFAULT_EXCLUDE.to_string()],
            config_path: opts.config_path.clone(),
            config_include: opts.config_include.clone(),
            config_exclude: opts.config_exclude.clone(),
        }
    }
}

/// Fold `rep.toml` scope defaults (unless `--no-config`) into freshly parsed
/// CLI options. Runs after root discovery because only the repository root
/// knows where the config lives.
pub fn resolve(root: &Path, mut opts: ScopeOpts) -> Result<ScopeOpts> {
    if opts.no_config {
        return Ok(opts);
    }
    if let Some(config) = crate::config::load(root)? {
        opts.config_path = Some(crate::config::CONFIG_FILE.to_string());
        opts.config_include = config.scope.include;
        opts.config_exclude = config.scope.exclude;
    }
    Ok(opts)
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

/// Reject any attempt to *include* `.rep/` (safety invariant).
///
/// Excluding `.rep/` is harmless (it is always excluded anyway), so only
/// `--include` is rejected.
pub fn reject_rep_dir(opts: &ScopeOpts) -> Result<()> {
    let targets_rep =
        |g: &str| g == schema::REP_DIR || g.starts_with(".rep/") || g.starts_with(".rep\\");
    for g in opts.include.iter() {
        if targets_rep(g) {
            return Err(RepError::InvalidArguments(format!(
                "--include '{g}' targets the reserved {} directory, which is always excluded",
                schema::REP_DIR
            )));
        }
    }
    for g in opts.config_include.iter() {
        if targets_rep(g) {
            return Err(RepError::InvalidArguments(format!(
                "{} include '{g}' targets the reserved {} directory, which is always excluded",
                crate::config::CONFIG_FILE,
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
        // When any include is given (CLI or config), a path must match at
        // least one glob from their union.
        let has_includes = !opts.include.is_empty() || !opts.config_include.is_empty();
        if has_includes
            && !globset::any_match(&opts.include, &path)
            && !globset::any_match(&opts.config_include, &path)
        {
            continue;
        }
        // CLI excludes are checked before config excludes so a rule present in
        // both is attributed to the explicit flag.
        if let Some(rule) = globset::first_match(&opts.exclude, &path) {
            skipped.push(Skip {
                path,
                reason: "excluded_by_glob".to_string(),
                matched_rule: Some(rule.to_string()),
            });
            continue;
        }
        if let Some(rule) = globset::first_match(&opts.config_exclude, &path) {
            skipped.push(Skip {
                path,
                reason: "excluded_by_config".to_string(),
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

/// Hex-encoded SHA-256 identifying a path's content.
///
/// For symlinks the link *target string* is hashed rather than following the
/// link, so a tracked symlink can be rename-planned without ever reading a file
/// that may live outside the repository.
pub fn sha256_file(path: &Path) -> Result<String> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(path)?;
        Ok(sha256_hex(target.to_string_lossy().as_bytes()))
    } else {
        Ok(sha256_hex(&std::fs::read(path)?))
    }
}
