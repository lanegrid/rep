//! `.rep/` artifact management: the plan data model plus reading and writing
//! plan directories and the active-state pointer.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{RepError, Result};
use crate::path_rename::Rename;
use crate::scope::{Scope, Skip};
use crate::text::Mapping;

/// Plan / status lifecycle states.
pub const STATE_PLANNED: &str = "planned";
pub const STATE_APPLIED: &str = "applied";
pub const STATE_FAILED: &str = "failed";
pub const STATE_NONE: &str = "none";

/// A content file touched by a plan, with its pre-change hash.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContentFile {
    pub path: String,
    pub sha256_before: String,
}

/// The content-replacement portion of a plan.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContentPlan {
    pub enabled: bool,
    pub matched_files: usize,
    pub changed_files: usize,
    pub replacements: usize,
    pub files: Vec<ContentFile>,
}

/// The path-rename portion of a plan.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PathsPlan {
    pub enabled: bool,
    pub matched_paths: usize,
    pub renames: Vec<Rename>,
}

/// Repository identity captured at plan time.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RepoInfo {
    pub root: String,
    pub git_head: String,
    pub tracked_tree_clean: bool,
}

/// Relative paths of a plan's sibling artifacts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Artifacts {
    pub summary: String,
    pub content_patch: String,
    pub path_renames: String,
    pub skipped: String,
}

/// The source of truth for a planned change.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plan {
    pub schema_version: String,
    pub plan_id: String,
    pub created_at: String,
    pub state: String,
    pub repo: RepoInfo,
    pub scope: Scope,
    pub mappings: Vec<Mapping>,
    pub content: ContentPlan,
    pub paths: PathsPlan,
    pub skipped: Vec<Skip>,
    pub artifacts: Artifacts,
}

/// The active-plan pointer stored at `.rep/state.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State {
    pub active_plan_id: String,
    pub state: String,
}

/// Path to the `.rep` directory for a repo root.
pub fn rep_dir(root: &Path) -> PathBuf {
    root.join(".rep")
}

/// Path to a specific plan directory.
pub fn plan_dir(root: &Path, plan_id: &str) -> PathBuf {
    rep_dir(root).join("plans").join(plan_id)
}

/// Build the relative artifact paths for a plan id.
pub fn artifact_paths(plan_id: &str) -> Artifacts {
    let base = format!(".rep/plans/{plan_id}");
    Artifacts {
        summary: format!("{base}/summary.json"),
        content_patch: format!("{base}/content.patch"),
        path_renames: format!("{base}/path-renames.json"),
        skipped: format!("{base}/skipped.json"),
    }
}

/// Write a plan and all of its sibling artifacts to disk.
pub fn write_plan(root: &Path, plan: &Plan, content_patch: &str, summary: &Summary) -> Result<()> {
    let dir = plan_dir(root, &plan.plan_id);
    std::fs::create_dir_all(&dir)?;
    write_json(&dir.join("plan.json"), plan)?;
    write_json(&dir.join("summary.json"), summary)?;
    write_json(&dir.join("path-renames.json"), &plan.paths.renames)?;
    write_json(&dir.join("skipped.json"), &plan.skipped)?;
    std::fs::write(dir.join("content.patch"), content_patch)?;
    Ok(())
}

/// Read a plan by id.
pub fn read_plan(root: &Path, plan_id: &str) -> Result<Plan> {
    let path = plan_dir(root, plan_id).join("plan.json");
    if !path.exists() {
        return Err(RepError::InvalidArguments(format!(
            "plan '{plan_id}' not found"
        )));
    }
    let data = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&data)?)
}

/// Update an existing plan.json in place.
pub fn update_plan(root: &Path, plan: &Plan) -> Result<()> {
    let path = plan_dir(root, &plan.plan_id).join("plan.json");
    write_json(&path, plan)
}

/// Read the active-state pointer, if present.
pub fn read_state(root: &Path) -> Result<Option<State>> {
    let path = rep_dir(root).join("state.json");
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(&path)?;
    Ok(Some(serde_json::from_str(&data)?))
}

/// Write the active-state pointer.
pub fn write_state(root: &Path, state: &State) -> Result<()> {
    std::fs::create_dir_all(rep_dir(root))?;
    write_json(&rep_dir(root).join("state.json"), state)
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let data = serde_json::to_string_pretty(value)?;
    std::fs::write(path, data)?;
    Ok(())
}

/// A lightweight, human-oriented summary written alongside the plan.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Summary {
    pub schema_version: String,
    pub plan_id: String,
    pub state: String,
    pub mappings: Vec<Mapping>,
    pub content_changed_files: usize,
    pub content_replacements: usize,
    pub path_renames: usize,
    pub skipped: usize,
}
