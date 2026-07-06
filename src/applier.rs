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

/// Execute `rep apply`. `plan_id: None` means `--last`: the most recent plan,
/// resolved from the state pointer.
pub fn run(plan_id: Option<String>, json: bool) -> Result<i32> {
    let root = git::discover_root()?;
    let plan_id = match plan_id {
        Some(id) => id,
        None => artifacts::last_plan_id(&root)?,
    };
    let mut plan = artifacts::read_plan(&root, &plan_id)?;

    // --- validation (all before any write) ---
    //
    // A failure that invalidates the plan itself (stale HEAD, changed file,
    // conflicting target) transitions the plan to `failed`. A dirty tracked
    // tree is a transient, external condition — the plan is still valid — so it
    // leaves the state as `planned`.
    if plan.repo.root != root.to_string_lossy() {
        let msg = format!(
            "plan was created for repo '{}', current repo is '{}'",
            plan.repo.root,
            root.display()
        );
        return fail(&root, &mut plan, RepError::StalePlan(msg));
    }

    let current_head = git::head(&root)?;
    if current_head != plan.repo.git_head {
        let msg = format!(
            "git HEAD changed since plan was created (plan: {}, now: {current_head})",
            plan.repo.git_head
        );
        return fail(&root, &mut plan, RepError::StalePlan(msg));
    }

    if !git::tracked_tree_clean(&root) {
        // Transient: keep state = planned so it can be retried after committing.
        return Err(RepError::TrackedTreeDirty);
    }

    // Verify before-hashes for every file the plan touches.
    let mut hash_mismatch: Option<String> = None;
    for f in &plan.content.files {
        if scope::sha256_file(&root.join(&f.path))? != f.sha256_before {
            hash_mismatch = Some(f.path.clone());
            break;
        }
    }
    if hash_mismatch.is_none() {
        for r in &plan.paths.renames {
            if scope::sha256_file(&root.join(&r.from))? != r.sha256_before {
                hash_mismatch = Some(r.from.clone());
                break;
            }
        }
    }
    if let Some(path) = hash_mismatch {
        return fail(&root, &mut plan, RepError::FileHashMismatch(path));
    }

    // Re-check that rename targets are still free.
    let tracked: HashSet<String> = git::tracked_set(&root)?;
    let mut conflict: Option<String> = None;
    for r in &plan.paths.renames {
        if tracked.contains(&r.to) && !plan.paths.renames.iter().any(|x| x.from == r.to) {
            conflict = Some(format!(
                "rename target '{}' now exists as a tracked file",
                r.to
            ));
            break;
        }
        if root.join(&r.to).exists() {
            conflict = Some(format!("rename target '{}' now exists on disk", r.to));
            break;
        }
    }
    if let Some(msg) = conflict {
        return fail(&root, &mut plan, RepError::PathConflict(msg));
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

/// Record the plan as failed and return the originating error.
fn fail(root: &std::path::Path, plan: &mut Plan, err: RepError) -> Result<i32> {
    mark_failed(root, plan);
    Err(err)
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
