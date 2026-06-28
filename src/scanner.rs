//! `rep scan` — measure how a token appears in tracked content and paths.
//!
//! Never modifies the working tree.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::error::Result;
use crate::output;
use crate::scope::{self, Scope, ScopeOpts, Skip};
use crate::{git, schema, text};

#[derive(Serialize)]
struct ContentReport {
    matched_files: usize,
    occurrences: usize,
    variants: BTreeMap<String, usize>,
    binary_skipped: usize,
    utf8_decode_skipped: usize,
}

#[derive(Serialize)]
struct PathsReport {
    matched_paths: usize,
    matched_directories: Vec<String>,
    matched_files: Vec<String>,
}

#[derive(Serialize)]
struct ScanReport {
    schema_version: String,
    token: String,
    case_insensitive: bool,
    scope: Scope,
    content: ContentReport,
    paths: PathsReport,
    skipped: Vec<Skip>,
}

/// Execute `rep scan`.
pub fn run(token: String, case_insensitive: bool, opts: ScopeOpts, json: bool) -> Result<i32> {
    let root = git::discover_root()?;
    scope::reject_rep_dir(&opts)?;
    let gathered = scope::gather(&root, &opts)?;

    // --- content scan ---
    let mut occurrences = 0;
    let mut matched_files = 0;
    let mut variants: BTreeMap<String, usize> = BTreeMap::new();
    for file in &gathered.files {
        let file_variants = text::variants(&file.content, &token, case_insensitive);
        if file_variants.is_empty() {
            continue;
        }
        matched_files += 1;
        for (surface, n) in file_variants {
            occurrences += n;
            *variants.entry(surface).or_insert(0) += n;
        }
    }

    // --- path scan ---
    let mut matched_path_files = Vec::new();
    let mut matched_dirs = std::collections::BTreeSet::new();
    for path in &gathered.all_paths {
        if !text::find_all(path, &token, case_insensitive).is_empty() {
            matched_path_files.push(path.clone());
            if let Some(parent) = std::path::Path::new(path).parent() {
                let parent = parent.to_string_lossy();
                if !parent.is_empty()
                    && !text::find_all(&parent, &token, case_insensitive).is_empty()
                {
                    matched_dirs.insert(parent.into_owned());
                }
            }
        }
    }

    let report = ScanReport {
        schema_version: schema::SCAN.to_string(),
        token: token.clone(),
        case_insensitive,
        scope: Scope::from_opts(&opts),
        content: ContentReport {
            matched_files,
            occurrences,
            variants,
            binary_skipped: gathered.binary_skipped,
            utf8_decode_skipped: gathered.utf8_decode_skipped,
        },
        paths: PathsReport {
            matched_paths: matched_path_files.len(),
            matched_directories: matched_dirs.into_iter().collect(),
            matched_files: matched_path_files.clone(),
        },
        skipped: gathered.skipped,
    };

    let no_matches = report.content.occurrences == 0 && report.paths.matched_paths == 0;

    if json {
        output::print_json(&report)?;
    } else {
        print_human(&report);
    }

    Ok(if no_matches { 2 } else { 0 })
}

fn print_human(report: &ScanReport) {
    output::info(&format!("scan {}", output::bold(&report.token)));
    println!("content:");
    if report.content.variants.is_empty() {
        println!("  (no occurrences)");
    } else {
        for (surface, n) in &report.content.variants {
            println!("  {surface}: {n}");
        }
    }
    println!(
        "  {} matched files, {} occurrences",
        report.content.matched_files, report.content.occurrences
    );
    println!("paths:");
    println!("  {} matched paths", report.paths.matched_paths);
}
