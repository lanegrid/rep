//! `rep apply` — apply a previously created plan to the working tree.
//!
//! Validates everything that can be validated before any write. On failure the
//! plan state is recorded as `failed`; recovery is delegated to git
//! (`git reset --hard HEAD`).

use std::collections::HashSet;

use serde::Serialize;

use crate::artifacts::{self, Plan, STATE_APPLIED, STATE_FAILED, State};
use crate::error::{RepError, Result};
use crate::output;
use crate::{git, schema, scope, text};

#[derive(Serialize)]
struct Validation {
    git_head_matched: bool,
    tracked_tree_clean: bool,
    file_hashes_matched: bool,
    path_conflicts: bool,
}

#[derive(Serialize)]
struct ContentResult {
    changed_files: usize,
    replacements: usize,
}

#[derive(Serialize)]
struct PathsResult {
    renamed_paths: usize,
}

#[derive(Serialize)]
struct NextStep {
    command: String,
}

#[derive(Serialize)]
struct ApplyOutput {
    schema_version: String,
    plan_id: String,
    state: String,
    content: ContentResult,
    paths: PathsResult,
    validation: Validation,
    next: Vec<NextStep>,
}

/// Execute `rep apply`.
pub fn run(plan_id: String, json: bool) -> Result<i32> {
    let root = git::discover_root()?;
    let mut plan = artifacts::read_plan(&root, &plan_id)?;

    // --- validation (all before any write) ---
    if plan.repo.root != root.to_string_lossy() {
        return Err(RepError::StalePlan(format!(
            "plan was created for repo '{}', current repo is '{}'",
            plan.repo.root,
            root.display()
        )));
    }

    let current_head = git::head(&root)?;
    let git_head_matched = current_head == plan.repo.git_head;
    if !git_head_matched {
        return Err(RepError::StalePlan(format!(
            "git HEAD changed since plan was created (plan: {}, now: {current_head})",
            plan.repo.git_head
        )));
    }

    if !git::tracked_tree_clean(&root) {
        return Err(RepError::TrackedTreeDirty);
    }

    // Verify before-hashes for every file the plan touches.
    for f in &plan.content.files {
        let actual = scope::sha256_file(&root.join(&f.path))?;
        if actual != f.sha256_before {
            return Err(RepError::FileHashMismatch(f.path.clone()));
        }
    }
    for r in &plan.paths.renames {
        let actual = scope::sha256_file(&root.join(&r.from))?;
        if actual != r.sha256_before {
            return Err(RepError::FileHashMismatch(r.from.clone()));
        }
    }

    // Re-check that rename targets are still free.
    let tracked: HashSet<String> = git::tracked_set(&root)?;
    for r in &plan.paths.renames {
        if tracked.contains(&r.to) && !plan.paths.renames.iter().any(|x| x.from == r.to) {
            return Err(RepError::PathConflict(format!(
                "rename target '{}' now exists as a tracked file",
                r.to
            )));
        }
        if root.join(&r.to).exists() {
            return Err(RepError::PathConflict(format!(
                "rename target '{}' now exists on disk",
                r.to
            )));
        }
    }

    // --- mutation ---
    if let Err(e) = apply_changes(&root, &plan) {
        mark_failed(&root, &mut plan);
        return Err(RepError::ApplyFailed(e.to_string()));
    }

    plan.state = STATE_APPLIED.to_string();
    artifacts::update_plan(&root, &plan)?;
    artifacts::write_state(
        &root,
        &State {
            active_plan_id: plan.plan_id.clone(),
            state: STATE_APPLIED.to_string(),
        },
    )?;

    let primary_token = plan
        .mappings
        .first()
        .map(|m| m.from.clone())
        .unwrap_or_default();

    let out = ApplyOutput {
        schema_version: schema::APPLY.to_string(),
        plan_id: plan.plan_id.clone(),
        state: STATE_APPLIED.to_string(),
        content: ContentResult {
            changed_files: plan.content.changed_files,
            replacements: plan.content.replacements,
        },
        paths: PathsResult {
            renamed_paths: plan.paths.renames.len(),
        },
        validation: Validation {
            git_head_matched: true,
            tracked_tree_clean: true,
            file_hashes_matched: true,
            path_conflicts: false,
        },
        next: vec![NextStep {
            command: format!(
                "rep residual {primary_token} --case-insensitive --tracked-only --json"
            ),
        }],
    };

    if json {
        output::print_json(&out)?;
    } else {
        print_human(&out);
    }

    Ok(0)
}

/// Perform content replacement first, then path renames.
fn apply_changes(root: &std::path::Path, plan: &Plan) -> Result<()> {
    if plan.content.enabled {
        for f in &plan.content.files {
            let full = root.join(&f.path);
            let content = std::fs::read_to_string(&full)?;
            let (new_content, _) = text::apply(&content, &plan.mappings);
            std::fs::write(&full, new_content)?;
        }
    }
    if plan.paths.enabled {
        for r in &plan.paths.renames {
            git::mv(root, &r.from, &r.to)?;
        }
    }
    Ok(())
}

fn mark_failed(root: &std::path::Path, plan: &mut Plan) {
    plan.state = STATE_FAILED.to_string();
    let _ = artifacts::update_plan(root, plan);
    let _ = artifacts::write_state(
        root,
        &State {
            active_plan_id: plan.plan_id.clone(),
            state: STATE_FAILED.to_string(),
        },
    );
}

fn print_human(out: &ApplyOutput) {
    output::success(&format!("applied plan {}", output::bold(&out.plan_id)));
    println!(
        "  content: {} replacements in {} files",
        out.content.replacements, out.content.changed_files
    );
    println!("  paths: {} renamed via git mv", out.paths.renamed_paths);
    if let Some(next) = out.next.first() {
        output::action(&next.command);
    }
}
