//! `rep status` — report the current rep state for the repository.

use serde::Serialize;

use crate::artifacts::{self, STATE_NONE};
use crate::error::Result;
use crate::output;
use crate::text::Mapping;
use crate::{git, schema};

#[derive(Serialize)]
struct PlanInfo {
    mappings: Vec<Mapping>,
    content_replacements: usize,
    path_renames: usize,
}

#[derive(Serialize)]
struct RepoInfo {
    tracked_tree_clean: bool,
}

#[derive(Serialize)]
struct NextStep {
    command: String,
}

#[derive(Serialize)]
struct StatusReport {
    schema_version: String,
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_plan_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan: Option<PlanInfo>,
    repo: RepoInfo,
    next: Vec<NextStep>,
}

/// Execute `rep status`.
pub fn run(json: bool) -> Result<i32> {
    let root = git::discover_root()?;
    let state = artifacts::read_state(&root)?;
    let tracked_tree_clean = git::tracked_tree_clean(&root);

    let report = match state {
        None => StatusReport {
            schema_version: schema::STATUS.to_string(),
            state: STATE_NONE.to_string(),
            active_plan_id: None,
            plan: None,
            repo: RepoInfo { tracked_tree_clean },
            next: vec![],
        },
        Some(state) => {
            let plan = artifacts::read_plan(&root, &state.active_plan_id).ok();
            let plan_info = plan.as_ref().map(|p| PlanInfo {
                mappings: p.mappings.clone(),
                content_replacements: p.content.replacements,
                path_renames: p.paths.renames.len(),
            });
            let next = next_steps(&state.state, &state.active_plan_id);
            StatusReport {
                schema_version: schema::STATUS.to_string(),
                state: state.state,
                active_plan_id: Some(state.active_plan_id),
                plan: plan_info,
                repo: RepoInfo { tracked_tree_clean },
                next,
            }
        }
    };

    if json {
        output::print_json(&report)?;
    } else {
        print_human(&report);
    }

    Ok(0)
}

fn next_steps(state: &str, plan_id: &str) -> Vec<NextStep> {
    match state {
        artifacts::STATE_PLANNED => vec![NextStep {
            command: format!("rep apply --plan {plan_id} --json"),
        }],
        artifacts::STATE_APPLIED => vec![NextStep {
            command: format!("rep residual --plan {plan_id} --json"),
        }],
        _ => vec![],
    }
}

fn print_human(report: &StatusReport) {
    output::info(&format!("state: {}", output::bold(&report.state)));
    if let Some(id) = &report.active_plan_id {
        println!("  active plan: {id}");
    }
    if let Some(plan) = &report.plan {
        println!(
            "  content replacements: {}, path renames: {}",
            plan.content_replacements, plan.path_renames
        );
    }
    for step in &report.next {
        output::action(&step.command);
    }
}
