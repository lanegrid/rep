//! `rep plan` — build a change plan from explicit literal mappings.
//!
//! Never modifies the working tree; all output lands under `.rep/plans/`.

use std::collections::HashMap;

use serde::Serialize;

use crate::artifacts::{
    self, Artifacts, ContentFile, ContentPlan, PathsPlan, Plan, RepoInfo, STATE_PLANNED, State,
    Summary,
};
use crate::error::Result;
use crate::output;
use crate::path_rename;
use crate::scope::{self, ScopeOpts};
use crate::{git, path_rename::Rename, schema, text};

#[derive(Serialize)]
struct PlanOutput {
    schema_version: String,
    plan_id: String,
    state: String,
    content: ContentSummary,
    paths: PathSummary,
    skipped: usize,
    working_tree_unchanged: bool,
    artifacts: Artifacts,
}

#[derive(Serialize)]
struct ContentSummary {
    matched_files: usize,
    changed_files: usize,
    replacements: usize,
}

#[derive(Serialize)]
struct PathSummary {
    matched_paths: usize,
    renames: usize,
}

/// Options for `rep plan`.
pub struct PlanOpts {
    pub maps: Vec<text::Mapping>,
    pub content: bool,
    pub rename_paths: bool,
    pub scope: ScopeOpts,
}

/// Execute `rep plan`.
pub fn run(opts: PlanOpts, json: bool) -> Result<i32> {
    let root = git::discover_root()?;
    scope::reject_rep_dir(&opts.scope)?;
    text::validate_mappings(&opts.maps)?;

    let git_head = git::head(&root)?;
    let tree_clean = git::tracked_tree_clean(&root);
    let gathered = scope::gather(&root, &opts.scope)?;

    // --- content operations ---
    let mut content_files: Vec<ContentFile> = Vec::new();
    let mut replacements = 0;
    let mut patch = String::new();
    if opts.content {
        for file in &gathered.files {
            let (new_content, n) = text::apply(&file.content, &opts.maps);
            if n == 0 {
                continue;
            }
            replacements += n;
            content_files.push(ContentFile {
                path: file.path.clone(),
                sha256_before: scope::sha256_hex(file.content.as_bytes()),
            });
            append_patch(&mut patch, &file.path, &file.content, &new_content);
        }
    }
    let changed_files = content_files.len();

    // --- path rename operations ---
    let mut renames: Vec<Rename> = Vec::new();
    if opts.rename_paths {
        let tracked = git::tracked_set(&root)?;
        let by_path: HashMap<&str, &str> = gathered
            .files
            .iter()
            .map(|f| (f.path.as_str(), f.content.as_str()))
            .collect();
        renames =
            path_rename::plan_renames(&root, &gathered.all_paths, &opts.maps, &tracked, |p| {
                match by_path.get(p) {
                    Some(content) => Ok(scope::sha256_hex(content.as_bytes())),
                    None => scope::sha256_file(&root.join(p)),
                }
            })?;
    }

    let plan_id = new_plan_id();
    let artifact_paths = artifacts::artifact_paths(&plan_id);
    let created_at = chrono::Local::now().to_rfc3339();

    let plan = Plan {
        schema_version: schema::PLAN.to_string(),
        plan_id: plan_id.clone(),
        created_at,
        state: STATE_PLANNED.to_string(),
        repo: RepoInfo {
            root: root.to_string_lossy().into_owned(),
            git_head,
            tracked_tree_clean: tree_clean,
        },
        scope: scope::Scope::from_opts(&opts.scope),
        mappings: opts.maps.clone(),
        content: ContentPlan {
            enabled: opts.content,
            matched_files: changed_files,
            changed_files,
            replacements,
            files: content_files,
        },
        paths: PathsPlan {
            enabled: opts.rename_paths,
            matched_paths: renames.len(),
            renames,
        },
        skipped: gathered.skipped,
        artifacts: artifact_paths.clone(),
    };

    let summary = Summary {
        schema_version: schema::PLAN.to_string(),
        plan_id: plan_id.clone(),
        state: STATE_PLANNED.to_string(),
        mappings: plan.mappings.clone(),
        content_changed_files: plan.content.changed_files,
        content_replacements: plan.content.replacements,
        path_renames: plan.paths.renames.len(),
        skipped: plan.skipped.len(),
    };

    artifacts::write_plan(&root, &plan, &patch, &summary)?;
    artifacts::write_state(
        &root,
        &State {
            active_plan_id: plan_id.clone(),
            state: STATE_PLANNED.to_string(),
        },
    )?;

    let out = PlanOutput {
        schema_version: schema::PLAN.to_string(),
        plan_id,
        state: STATE_PLANNED.to_string(),
        content: ContentSummary {
            matched_files: plan.content.matched_files,
            changed_files: plan.content.changed_files,
            replacements: plan.content.replacements,
        },
        paths: PathSummary {
            matched_paths: plan.paths.matched_paths,
            renames: plan.paths.renames.len(),
        },
        skipped: plan.skipped.len(),
        working_tree_unchanged: true,
        artifacts: artifact_paths,
    };

    if json {
        output::print_json(&out)?;
    } else {
        print_human(&out);
    }

    Ok(0)
}

/// Generate a sortable, timestamp-based plan id.
fn new_plan_id() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H-%M-%S").to_string()
}

/// Append a minimal line-level diff for a changed file to the content patch.
fn append_patch(patch: &mut String, path: &str, before: &str, after: &str) {
    patch.push_str(&format!("--- a/{path}\n+++ b/{path}\n"));
    for (i, (old, new)) in before.lines().zip(after.lines()).enumerate() {
        if old != new {
            patch.push_str(&format!("@@ line {} @@\n-{old}\n+{new}\n", i + 1));
        }
    }
}

fn print_human(out: &PlanOutput) {
    output::success(&format!("plan {}", output::bold(&out.plan_id)));
    println!(
        "  content replacements: {} ({} changed files)",
        out.content.replacements, out.content.changed_files
    );
    println!("  path renames: {}", out.paths.renames);
    println!("  working tree unchanged");
    output::action(&format!("rep apply --plan {} --json", out.plan_id));
}
