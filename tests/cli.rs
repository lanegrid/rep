//! End-to-end CLI tests driving the built `rep` binary against temp git repos.
//!
//! These mirror the acceptance criteria from the specification (AC1..AC10) plus
//! the minimal success example.

use std::path::Path;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

fn rep_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rep")
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .expect("failed to run git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

struct RunResult {
    code: i32,
    stdout: String,
}

impl RunResult {
    fn json(&self) -> Value {
        serde_json::from_str(&self.stdout)
            .unwrap_or_else(|e| panic!("invalid JSON: {e}\n--- stdout ---\n{}", self.stdout))
    }
}

fn rep(dir: &Path, args: &[&str]) -> RunResult {
    let out = Command::new(rep_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run rep");
    RunResult {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
    }
}

fn write(dir: &Path, rel: &str, content: &str) {
    let full = dir.join(rel);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(full, content).unwrap();
}

fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).unwrap()
}

fn init_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "test"]);
    dir
}

fn tree_clean(dir: &Path) -> bool {
    let unstaged = Command::new("git")
        .args(["diff", "--quiet"])
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    let staged = Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    unstaged && staged
}

/// Set up the minimal success example from the spec.
fn setup_success_example() -> TempDir {
    let dir = init_repo();
    write(
        dir.path(),
        "src/oldname.ts",
        "export const OLDNAME_DATA_ROOT = \"~/Movies/oldname\"\nexport class OldNameClient {}\n",
    );
    write(dir.path(), "README.md", "# project\n");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "init"]);
    dir
}

fn plan_three_maps(dir: &Path, extra: &[&str]) -> String {
    let mut args = vec![
        "plan",
        "--map",
        "oldname=newname",
        "--map",
        "OldName=NewName",
        "--map",
        "OLDNAME=NEWNAME",
        "--json",
    ];
    args.extend_from_slice(extra);
    let res = rep(dir, &args);
    assert_eq!(res.code, 0, "plan failed: {}", res.stdout);
    res.json()["plan_id"].as_str().unwrap().to_string()
}

// --- AC1: scan does not modify the working tree ---
#[test]
fn ac1_scan_does_not_modify_tree() {
    let dir = setup_success_example();
    let res = rep(
        dir.path(),
        &["scan", "oldname", "--case-insensitive", "--json"],
    );
    assert_eq!(res.code, 0);
    assert!(tree_clean(dir.path()));
    let json = res.json();
    assert_eq!(json["schema_version"], "rep.scan.v1");
    // Two case variants present in content (OLDNAME, OldName).
    let variants = &json["content"]["variants"];
    assert!(variants.get("OLDNAME").is_some());
    assert!(variants.get("OldName").is_some());
}

// --- AC2: plan does not modify the tree and writes plan.json ---
#[test]
fn ac2_plan_does_not_modify_tree() {
    let dir = setup_success_example();
    let plan_id = plan_three_maps(dir.path(), &[]);
    assert!(tree_clean(dir.path()));
    let plan_path = dir.path().join(format!(".rep/plans/{plan_id}/plan.json"));
    assert!(plan_path.exists(), "plan.json should exist");
}

// --- AC4: only tracked files are changed ---
#[test]
fn ac4_untracked_files_untouched() {
    let dir = setup_success_example();
    write(dir.path(), "untracked.txt", "oldname here\n");
    let plan_id = plan_three_maps(dir.path(), &["--rename-paths"]);
    let res = rep(dir.path(), &["apply", "--plan", &plan_id, "--json"]);
    assert_eq!(res.code, 0, "apply failed: {}", res.stdout);
    assert_eq!(read(dir.path(), "untracked.txt"), "oldname here\n");
}

// --- AC5: .rep is never in scope ---
#[test]
fn ac5_rep_dir_excluded() {
    let dir = setup_success_example();
    write(
        dir.path(),
        ".rep/plans/old/plan.json",
        "oldname inside rep\n",
    );
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "add rep"]);
    let res = rep(dir.path(), &["scan", "oldname", "--json"]);
    let json = res.json();
    // The .rep file's "oldname" must not be counted...
    let files = json["content"]["matched_files"].as_u64().unwrap();
    assert_eq!(files, 1, "only src/oldname.ts should match, not .rep");
    // ...and it should be reported as skipped with the rep_internal reason.
    let skipped = json["skipped"].as_array().unwrap();
    assert!(skipped.iter().any(|s| s["reason"] == "rep_internal"));
}

// --- AC6: path conflict fails plan with exit code 6 ---
#[test]
fn ac6_path_conflict_fails_plan() {
    let dir = init_repo();
    write(dir.path(), "a/oldname.txt", "x\n");
    write(dir.path(), "a/newname.txt", "y\n");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "init"]);
    let res = rep(
        dir.path(),
        &[
            "plan",
            "--map",
            "oldname=newname",
            "--rename-paths",
            "--json",
        ],
    );
    assert_eq!(res.code, 6, "expected path conflict exit code");
}

// --- AC7: file hash mismatch fails apply with exit code 7 ---
#[test]
fn ac7_hash_mismatch_fails_apply() {
    let dir = setup_success_example();
    // Make the working tree dirty so the plan records the dirty content's hash.
    write(
        dir.path(),
        "src/oldname.ts",
        "export class OldNameClient {} // oldname extra\n",
    );
    let plan_id = plan_three_maps(dir.path(), &[]);
    // Restore the file: tree clean + HEAD unchanged, but content != plan hash.
    git(dir.path(), &["checkout", "--", "src/oldname.ts"]);
    assert!(tree_clean(dir.path()));
    let res = rep(dir.path(), &["apply", "--plan", &plan_id, "--json"]);
    assert_eq!(res.code, 7, "expected hash mismatch exit code");
}

// --- AC6 variant via apply: dirty tree fails apply with exit code 4 ---
#[test]
fn ac6_dirty_tree_fails_apply() {
    let dir = setup_success_example();
    let plan_id = plan_three_maps(dir.path(), &[]);
    // Dirty the tree after planning.
    write(dir.path(), "README.md", "# project changed\n");
    assert!(!tree_clean(dir.path()));
    let res = rep(dir.path(), &["apply", "--plan", &plan_id, "--json"]);
    assert_eq!(res.code, 4, "expected dirty tree exit code");
}

// --- AC8: residual sees content AND path; path-only residual fails (exit 8) ---
#[test]
fn ac8_residual_detects_path_only() {
    let dir = init_repo();
    write(dir.path(), "oldname.txt", "clean content\n");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "init"]);
    let res = rep(dir.path(), &["residual", "oldname", "--json"]);
    let json = res.json();
    assert_eq!(json["content"]["occurrences"], 0);
    assert_eq!(json["paths"]["occurrences"], 1);
    assert_eq!(json["passed"], false);
    assert_eq!(res.code, 8);
}

// --- AC10 + minimal success example: full pipeline ---
#[test]
fn ac10_full_pipeline_success() {
    let dir = setup_success_example();

    // scan
    let scan = rep(
        dir.path(),
        &["scan", "oldname", "--case-insensitive", "--json"],
    );
    assert_eq!(scan.code, 0);

    // plan with content + path rename
    let plan_id = plan_three_maps(dir.path(), &["--rename-paths"]);

    // status -> planned
    let status = rep(dir.path(), &["status", "--json"]);
    assert_eq!(status.json()["state"], "planned");

    // apply
    let apply = rep(dir.path(), &["apply", "--plan", &plan_id, "--json"]);
    assert_eq!(apply.code, 0, "apply failed: {}", apply.stdout);
    assert_eq!(apply.json()["state"], "applied");

    // file renamed + content rewritten
    assert!(dir.path().join("src/newname.ts").exists());
    assert!(!dir.path().join("src/oldname.ts").exists());
    let content = read(dir.path(), "src/newname.ts");
    assert!(content.contains("NEWNAME_DATA_ROOT"));
    assert!(content.contains("NewNameClient"));
    assert!(content.contains("~/Movies/newname"));

    // residual -> clean
    let residual = rep(
        dir.path(),
        &["residual", "oldname", "--case-insensitive", "--json"],
    );
    assert_eq!(residual.code, 0, "residual: {}", residual.stdout);
    assert_eq!(residual.json()["passed"], true);

    // residual by plan -> clean
    let residual_plan = rep(dir.path(), &["residual", "--plan", &plan_id, "--json"]);
    assert_eq!(residual_plan.code, 0);

    // status -> applied
    let status = rep(dir.path(), &["status", "--json"]);
    assert_eq!(status.json()["state"], "applied");
    assert_eq!(status.json()["active_plan_id"], plan_id);
}

// --- scan with no matches returns exit code 2 ---
#[test]
fn scan_no_matches_exit_2() {
    let dir = setup_success_example();
    let res = rep(dir.path(), &["scan", "zzz-absent-token", "--json"]);
    assert_eq!(res.code, 2);
}

// --- not a git repository returns exit code 3 ---
#[test]
fn not_a_git_repo_exit_3() {
    let dir = TempDir::new().unwrap();
    let res = rep(dir.path(), &["status", "--json"]);
    assert_eq!(res.code, 3);
}

// --- ambiguous mappings rejected with exit code 10 ---
#[test]
fn ambiguous_mappings_exit_10() {
    let dir = setup_success_example();
    let res = rep(
        dir.path(),
        &["plan", "--map", "foo=bar", "--map", "bar=baz", "--json"],
    );
    assert_eq!(res.code, 10);
}

// --- including .rep is rejected with exit code 10 ---
#[test]
fn include_rep_rejected_exit_10() {
    let dir = setup_success_example();
    let res = rep(
        dir.path(),
        &["scan", "oldname", "--include", ".rep/**", "--json"],
    );
    assert_eq!(res.code, 10);
}

// --- D1: --json failures emit a machine-readable error envelope ---
#[test]
fn d1_json_error_output() {
    let dir = setup_success_example();
    let plan_id = plan_three_maps(dir.path(), &[]);
    write(dir.path(), "README.md", "# dirtied\n"); // make tree dirty
    let res = rep(dir.path(), &["apply", "--plan", &plan_id, "--json"]);
    assert_eq!(res.code, 4);
    let json = res.json();
    assert_eq!(json["schema_version"], "rep.error.v1");
    assert_eq!(json["error"]["kind"], "tracked_tree_dirty");
    assert_eq!(json["error"]["exit_code"], 4);
}

// --- D2: stale/hash failures record state=failed; dirty stays planned ---
#[test]
fn d2_hash_mismatch_marks_failed() {
    let dir = setup_success_example();
    write(
        dir.path(),
        "src/oldname.ts",
        "export class OldNameClient {} // oldname extra\n",
    );
    let plan_id = plan_three_maps(dir.path(), &[]);
    git(dir.path(), &["checkout", "--", "src/oldname.ts"]);
    let res = rep(dir.path(), &["apply", "--plan", &plan_id, "--json"]);
    assert_eq!(res.code, 7);
    let status = rep(dir.path(), &["status", "--json"]);
    assert_eq!(status.json()["state"], "failed");
}

#[test]
fn d2_dirty_tree_stays_planned() {
    let dir = setup_success_example();
    let plan_id = plan_three_maps(dir.path(), &[]);
    write(dir.path(), "README.md", "# dirtied\n");
    let res = rep(dir.path(), &["apply", "--plan", &plan_id, "--json"]);
    assert_eq!(res.code, 4);
    // A transient dirty tree must not mark the plan failed.
    let status = rep(dir.path(), &["status", "--json"]);
    assert_eq!(status.json()["state"], "planned");
}

// --- D3: case-only rename is rejected with exit code 6 ---
#[test]
fn d3_case_only_rename_rejected() {
    let dir = init_repo();
    write(dir.path(), "Foo.txt", "x\n");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "init"]);
    let res = rep(
        dir.path(),
        &["plan", "--map", "Foo=foo", "--rename-paths", "--json"],
    );
    assert_eq!(res.code, 6);
}

// --- D4: a no-op plan returns exit code 2 and writes no artifacts ---
#[test]
fn d4_no_op_plan_exit_2() {
    let dir = init_repo();
    write(dir.path(), "hello.txt", "hello world\n");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "init"]);
    let res = rep(dir.path(), &["plan", "--map", "absent=present", "--json"]);
    assert_eq!(res.code, 2);
    assert!(!dir.path().join(".rep/plans").exists());
}

// --- D5: the content preview artifact is named content-preview.txt ---
#[test]
fn d5_content_preview_artifact_name() {
    let dir = setup_success_example();
    let plan_id = plan_three_maps(dir.path(), &[]);
    let base = dir.path().join(format!(".rep/plans/{plan_id}"));
    assert!(base.join("content-preview.txt").exists());
    assert!(!base.join("content.patch").exists());
}

// --- D6: distinct plans get distinct ids and directories ---
#[test]
fn d6_plan_ids_are_unique() {
    let dir = setup_success_example();
    let id1 = plan_three_maps(dir.path(), &[]);
    let id2 = plan_three_maps(dir.path(), &[]);
    assert_ne!(id1, id2);
    assert!(
        dir.path()
            .join(format!(".rep/plans/{id1}/plan.json"))
            .exists()
    );
    assert!(
        dir.path()
            .join(format!(".rep/plans/{id2}/plan.json"))
            .exists()
    );
}

// --- D7: a tracked symlink can be rename-planned without reading its target ---
#[cfg(unix)]
#[test]
fn d7_symlink_rename_does_not_follow_target() {
    use std::os::unix::fs::symlink;
    let dir = init_repo();
    // Symlink whose target lives outside the repo and does not exist.
    symlink(
        "/nonexistent/outside/oldname-target",
        dir.path().join("oldname-link"),
    )
    .unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "init"]);
    let res = rep(
        dir.path(),
        &[
            "plan",
            "--map",
            "oldname=newname",
            "--rename-paths",
            "--json",
        ],
    );
    assert_eq!(res.code, 0, "plan failed: {}", res.stdout);
    let plan_id = res.json()["plan_id"].as_str().unwrap().to_string();
    let apply = rep(dir.path(), &["apply", "--plan", &plan_id, "--json"]);
    assert_eq!(apply.code, 0, "apply failed: {}", apply.stdout);
    // The target is a dangling symlink, so use symlink_metadata (exists() follows links).
    assert!(std::fs::symlink_metadata(dir.path().join("newname-link")).is_ok());
    assert!(std::fs::symlink_metadata(dir.path().join("oldname-link")).is_err());
}

// --- D8: excluding .rep is allowed (only --include is rejected) ---
#[test]
fn d8_exclude_rep_allowed() {
    let dir = setup_success_example();
    let res = rep(
        dir.path(),
        &["scan", "oldname", "--exclude", ".rep/**", "--json"],
    );
    assert_eq!(res.code, 0);
}

// --- D9: matched_directories reports the token-bearing directory prefix ---
#[test]
fn d9_matched_directory_prefix() {
    let dir = init_repo();
    write(
        dir.path(),
        "packages/oldname-core/src/index.ts",
        "content\n",
    );
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "init"]);
    let res = rep(dir.path(), &["scan", "oldname", "--json"]);
    let dirs = res.json()["paths"]["matched_directories"].clone();
    let dirs = dirs.as_array().unwrap();
    assert!(dirs.iter().any(|d| d == "packages/oldname-core"));
}
