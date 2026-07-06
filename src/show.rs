//! `rep show` — inspect a saved plan without reading `.rep/plans/<plan-id>/`
//! files by hand.
//!
//! The summary (mappings, counts, state, next step) always prints; `--files`,
//! `--skipped`, and `--preview` add the corresponding detail sections.

use serde::Serialize;

use crate::artifacts::{self, Artifacts, ContentFile, DerivedInfo, NextStep, RepoInfo};
use crate::error::Result;
use crate::output;
use crate::path_rename::Rename;
use crate::scope::{Scope, Skip};
use crate::text::Mapping;
use crate::{git, schema};

#[derive(Serialize)]
struct ContentSummary {
    enabled: bool,
    matched_files: usize,
    changed_files: usize,
    replacements: usize,
}

#[derive(Serialize)]
struct PathsSummary {
    enabled: bool,
    matched_paths: usize,
    renames: usize,
}

#[derive(Serialize)]
struct FilesSection {
    content: Vec<ContentFile>,
    renames: Vec<Rename>,
}

#[derive(Serialize)]
struct ShowOutput {
    schema_version: String,
    plan_id: String,
    created_at: String,
    state: String,
    repo: RepoInfo,
    scope: Scope,
    mappings: Vec<Mapping>,
    #[serde(skip_serializing_if = "Option::is_none")]
    derived: Option<DerivedInfo>,
    content: ContentSummary,
    paths: PathsSummary,
    skipped_count: usize,
    artifacts: Artifacts,
    next: Vec<NextStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    files: Option<FilesSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    skipped: Option<Vec<Skip>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preview: Option<String>,
}

/// Options for `rep show`. `plan: None` means the most recent plan (the one
/// the state pointer names), which is also what bare `rep show` does.
pub struct ShowOpts {
    pub plan: Option<String>,
    pub files: bool,
    pub skipped: bool,
    pub preview: bool,
}

/// Execute `rep show`.
pub fn run(opts: ShowOpts, json: bool) -> Result<i32> {
    let root = git::discover_root()?;
    let plan_id = match opts.plan {
        Some(id) => id,
        None => artifacts::last_plan_id(&root)?,
    };
    let plan = artifacts::read_plan(&root, &plan_id)?;

    let preview = if opts.preview {
        Some(std::fs::read_to_string(
            root.join(&plan.artifacts.content_preview),
        )?)
    } else {
        None
    };

    let out = ShowOutput {
        schema_version: schema::SHOW.to_string(),
        plan_id: plan.plan_id.clone(),
        created_at: plan.created_at,
        state: plan.state.clone(),
        repo: plan.repo,
        scope: plan.scope,
        mappings: plan.mappings,
        derived: plan.derived,
        content: ContentSummary {
            enabled: plan.content.enabled,
            matched_files: plan.content.matched_files,
            changed_files: plan.content.changed_files,
            replacements: plan.content.replacements,
        },
        paths: PathsSummary {
            enabled: plan.paths.enabled,
            matched_paths: plan.paths.matched_paths,
            renames: plan.paths.renames.len(),
        },
        skipped_count: plan.skipped.len(),
        artifacts: plan.artifacts,
        next: artifacts::next_steps(&plan.state, &plan.plan_id),
        files: opts.files.then_some(FilesSection {
            content: plan.content.files,
            renames: plan.paths.renames,
        }),
        skipped: opts.skipped.then_some(plan.skipped),
        preview,
    };

    if json {
        output::print_json(&out)?;
    } else {
        print_human(&out);
    }

    Ok(0)
}

fn print_human(out: &ShowOutput) {
    output::info(&format!(
        "plan {} ({})",
        output::bold(&out.plan_id),
        out.state
    ));
    let short_head: String = out.repo.git_head.chars().take(12).collect();
    println!("  created: {}   head: {}", out.created_at, short_head);
    println!("  mappings:");
    for m in &out.mappings {
        println!("    {} -> {}", m.from, m.to);
    }
    if let Some(derived) = &out.derived {
        println!(
            "  derived from staged git renames: {} mappings",
            derived.mappings.len()
        );
        for u in &derived.underivable {
            println!("    underivable: {} -> {} ({})", u.from, u.to, u.reason);
        }
    }
    println!(
        "  content: {} replacements in {} files",
        out.content.replacements, out.content.changed_files
    );
    println!("  paths: {} renames", out.paths.renames);
    println!("  skipped: {}", out.skipped_count);
    if let Some(files) = &out.files {
        println!("  files:");
        for f in &files.content {
            println!("    {}", f.path);
        }
        for r in &files.renames {
            println!("    {} -> {}", r.from, r.to);
        }
    }
    if let Some(skipped) = &out.skipped {
        println!("  skipped paths:");
        for s in skipped {
            match &s.matched_rule {
                Some(rule) => println!("    {} ({}: {})", s.path, s.reason, rule),
                None => println!("    {} ({})", s.path, s.reason),
            }
        }
    }
    if let Some(preview) = &out.preview {
        print!("{preview}");
    }
    for step in &out.next {
        output::action(&step.command);
    }
}
