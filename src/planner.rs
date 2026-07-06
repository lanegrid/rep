//! `rep plan` — build a change plan from explicit literal mappings.
//!
//! Never modifies the working tree; all output lands under `.rep/plans/`.

use std::collections::HashMap;

use serde::Serialize;

use crate::artifacts::{
    self, Artifacts, ContentFile, ContentPlan, DerivedInfo, PathsPlan, Plan, RepoInfo,
    STATE_PLANNED, State, Summary,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    derived: Option<DerivedInfo>,
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
    /// Set when `--from-git-renames` ran: the derived mappings (already merged
    /// into `maps`) and the staged renames nothing could be derived from.
    pub derived: Option<DerivedInfo>,
    pub content: bool,
    pub rename_paths: bool,
    pub scope: ScopeOpts,
}

/// Execute `rep plan`.
pub fn run(mut opts: PlanOpts, json: bool) -> Result<i32> {
    let root = git::discover_root()?;
    opts.scope = scope::resolve(&root, opts.scope)?;
    scope::reject_rep_dir(&opts.scope)?;

    // `--from-git-renames` alone may legitimately derive nothing (no staged
    // renames, or only underivable ones). That is "no matches" (exit 2), not a
    // usage error — and the underivable list must still reach the caller.
    if opts.maps.is_empty() && opts.derived.is_some() {
        return no_op(&opts, json);
    }
    text::validate_mappings(&opts.maps)?;

    let git_head = git::head(&root)?;
    let tree_clean = git::tracked_tree_clean(&root);
    let gathered = scope::gather(&root, &opts.scope)?;

    // --- content operations ---
    let mut content_files: Vec<ContentFile> = Vec::new();
    let mut replacements = 0;
    let mut preview = String::new();
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
            append_preview(&mut preview, &file.path, &file.content, &new_content);
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

    // A plan that would change nothing is reported as "no matches" (exit 2) and
    // no artifacts are written, so agents don't mistake it for real work.
    if changed_files == 0 && renames.is_empty() {
        return no_op(&opts, json);
    }

    let plan_id = unique_plan_id(&root);
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
        derived: opts.derived.clone(),
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

    artifacts::write_plan(&root, &plan, &preview, &summary)?;
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
        derived: opts.derived,
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

/// Report a plan that would change nothing: "no matches" (exit 2), no
/// artifacts written. When `--from-git-renames` ran, the derived mappings and
/// underivable renames are included so they are never silently dropped.
fn no_op(opts: &PlanOpts, json: bool) -> Result<i32> {
    if json {
        let mut out = serde_json::json!({
            "schema_version": schema::PLAN,
            "state": "none",
            "no_op": true,
            "content": { "changed_files": 0, "replacements": 0 },
            "paths": { "renames": 0 },
        });
        if let Some(derived) = &opts.derived {
            out["derived"] = serde_json::to_value(derived)?;
        }
        output::print_json(&out)?;
    } else {
        match &opts.derived {
            Some(derived) if opts.maps.is_empty() => {
                output::warn("no mappings derivable from staged renames");
                for u in &derived.underivable {
                    println!("  {} -> {} ({})", u.from, u.to, u.reason);
                }
                if derived.underivable.is_empty() {
                    output::action(
                        "stage renames first (git mv OLD NEW), then re-run rep plan --from-git-renames",
                    );
                } else {
                    output::action("describe these renames with explicit --map FROM=TO entries");
                }
            }
            _ => output::warn("no changes to plan for the given mappings"),
        }
    }
    Ok(2)
}

/// Generate a sortable, timestamp-based plan id, adding a numeric suffix if a
/// plan with that id already exists (agents may create plans within the same
/// second).
fn unique_plan_id(root: &std::path::Path) -> String {
    let base = chrono::Local::now().format("%Y-%m-%dT%H-%M-%S").to_string();
    if !artifacts::plan_dir(root, &base).exists() {
        return base;
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if !artifacts::plan_dir(root, &candidate).exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Append a human-oriented, line-level preview for a changed file. This is a
/// *preview only* — not a `git apply`-able patch — hence the `.txt` artifact.
fn append_preview(preview: &mut String, path: &str, before: &str, after: &str) {
    preview.push_str(&format!("# {path}\n"));
    for (i, (old, new)) in before.lines().zip(after.lines()).enumerate() {
        if old != new {
            preview.push_str(&format!("@@ line {} @@\n- {old}\n+ {new}\n", i + 1));
        }
    }
    preview.push('\n');
}

fn print_human(out: &PlanOutput) {
    output::success(&format!("plan {}", output::bold(&out.plan_id)));
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
        "  content replacements: {} ({} changed files)",
        out.content.replacements, out.content.changed_files
    );
    println!("  path renames: {}", out.paths.renames);
    println!("  working tree unchanged");
    output::action(&format!("rep apply --plan {} --json", out.plan_id));
}
