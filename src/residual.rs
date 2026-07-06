//! `rep residual` — confirm an old token no longer appears in tracked content
//! or paths after a rename.

use serde::Serialize;

use crate::artifacts;
use crate::error::{RepError, Result};
use crate::output;
use crate::scope::{self, Scope, ScopeOpts};
use crate::{git, schema, text};

#[derive(Serialize)]
struct ContentHit {
    path: String,
    line: usize,
    preview: String,
}

#[derive(Serialize)]
struct ContentReport {
    occurrences: usize,
    files: Vec<ContentHit>,
}

#[derive(Serialize)]
struct PathsReport {
    occurrences: usize,
    files: Vec<String>,
}

#[derive(Serialize)]
struct ResidualReport {
    schema_version: String,
    tokens: Vec<String>,
    case_insensitive: bool,
    scope: Scope,
    content: ContentReport,
    paths: PathsReport,
    passed: bool,
}

/// Options for `rep residual`.
pub struct ResidualOpts {
    pub token: Option<String>,
    pub plan: Option<String>,
    /// Derive tokens from the most recent plan (state pointer) instead of an
    /// explicit `--plan` id or positional token.
    pub last: bool,
    pub case_insensitive: bool,
    pub scope: ScopeOpts,
}

/// Execute `rep residual`.
pub fn run(mut opts: ResidualOpts, json: bool) -> Result<i32> {
    let root = git::discover_root()?;
    opts.scope = scope::resolve(&root, opts.scope)?;
    scope::reject_rep_dir(&opts.scope)?;

    let tokens = resolve_tokens(&root, &opts)?;
    let gathered = scope::gather(&root, &opts.scope)?;

    // --- content residual ---
    let mut content_occurrences = 0;
    let mut content_hits = Vec::new();
    for file in &gathered.files {
        for (idx, line) in file.content.lines().enumerate() {
            let n: usize = tokens
                .iter()
                .map(|t| text::count(line, t, opts.case_insensitive))
                .sum();
            if n > 0 {
                content_occurrences += n;
                content_hits.push(ContentHit {
                    path: file.path.clone(),
                    line: idx + 1,
                    preview: line.trim().to_string(),
                });
            }
        }
    }

    // --- path residual ---
    let mut path_files = Vec::new();
    for path in &gathered.all_paths {
        let hit = tokens
            .iter()
            .any(|t| !text::find_all(path, t, opts.case_insensitive).is_empty());
        if hit {
            path_files.push(path.clone());
        }
    }

    let passed = content_occurrences == 0 && path_files.is_empty();

    let report = ResidualReport {
        schema_version: schema::RESIDUAL.to_string(),
        tokens,
        case_insensitive: opts.case_insensitive,
        scope: Scope::from_opts(&opts.scope),
        content: ContentReport {
            occurrences: content_occurrences,
            files: content_hits,
        },
        paths: PathsReport {
            occurrences: path_files.len(),
            files: path_files,
        },
        passed,
    };

    if json {
        output::print_json(&report)?;
    } else {
        print_human(&report);
    }

    Ok(if passed { 0 } else { 8 })
}

/// Resolve the tokens to check: each mapping `from` for `--plan`/`--last`,
/// otherwise the single positional token.
fn resolve_tokens(root: &std::path::Path, opts: &ResidualOpts) -> Result<Vec<String>> {
    let plan_id = if opts.last {
        Some(artifacts::last_plan_id(root)?)
    } else {
        opts.plan.clone()
    };
    if let Some(plan_id) = &plan_id {
        let plan = artifacts::read_plan(root, plan_id)?;
        Ok(plan.mappings.into_iter().map(|m| m.from).collect())
    } else if let Some(token) = &opts.token {
        Ok(vec![token.clone()])
    } else {
        Err(RepError::InvalidArguments(
            "either a TOKEN, --plan PLAN_ID, or --last is required".to_string(),
        ))
    }
}

fn print_human(report: &ResidualReport) {
    if report.passed {
        output::success(&format!("residual clean for {:?}", report.tokens));
    } else {
        output::warn(&format!("residual found for {:?}", report.tokens));
    }
    println!("  content occurrences: {}", report.content.occurrences);
    println!("  path occurrences: {}", report.paths.occurrences);
    println!("  passed: {}", report.passed);
}
