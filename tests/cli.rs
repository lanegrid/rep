//! End-to-end CLI tests driving the built `rep` binary against temp git repos.
//!
//! Each test names the behavior it verifies and is self-contained.

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

fn rep_with_stdin(dir: &Path, args: &[&str], stdin: &str) -> RunResult {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(rep_bin())
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to run rep");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().expect("failed to run rep");
    RunResult {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
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

// scan does not modify the working tree
#[test]
fn scan_does_not_modify_tree() {
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

// plan does not modify the tree and writes plan.json
#[test]
fn plan_does_not_modify_tree() {
    let dir = setup_success_example();
    let plan_id = plan_three_maps(dir.path(), &[]);
    assert!(tree_clean(dir.path()));
    let plan_path = dir.path().join(format!(".rep/plans/{plan_id}/plan.json"));
    assert!(plan_path.exists(), "plan.json should exist");
}

// only tracked files are changed; untracked files are left alone
#[test]
fn untracked_files_untouched() {
    let dir = setup_success_example();
    write(dir.path(), "untracked.txt", "oldname here\n");
    let plan_id = plan_three_maps(dir.path(), &["--rename-paths"]);
    let res = rep(dir.path(), &["apply", "--plan", &plan_id, "--json"]);
    assert_eq!(res.code, 0, "apply failed: {}", res.stdout);
    assert_eq!(read(dir.path(), "untracked.txt"), "oldname here\n");
}

// .rep is never in scope
#[test]
fn rep_dir_excluded() {
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

// a path rename conflict fails plan with exit code 6
#[test]
fn path_conflict_fails_plan() {
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

// a file hash mismatch fails apply with exit code 7
#[test]
fn hash_mismatch_fails_apply() {
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

// a dirty tracked tree fails apply with exit code 4
#[test]
fn dirty_tree_fails_apply() {
    let dir = setup_success_example();
    let plan_id = plan_three_maps(dir.path(), &[]);
    // Dirty the tree after planning.
    write(dir.path(), "README.md", "# project changed\n");
    assert!(!tree_clean(dir.path()));
    let res = rep(dir.path(), &["apply", "--plan", &plan_id, "--json"]);
    assert_eq!(res.code, 4, "expected dirty tree exit code");
}

// residual checks both content and path; a path-only hit fails (exit 8)
#[test]
fn residual_detects_path_only() {
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

// the full scan -> plan -> apply -> residual -> status pipeline succeeds
#[test]
fn full_pipeline_success() {
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

// --- clap usage errors map to invalid-arguments (exit 10), not clap's
// default 2, so agents can tell a mistyped command from an empty scan ---
#[test]
fn unrecognized_subcommand_exit_10() {
    let dir = init_repo();
    let res = rep(dir.path(), &["badcmd"]);
    assert_eq!(res.code, 10);
}

#[test]
fn missing_required_argument_exit_10() {
    let dir = init_repo();
    let res = rep(dir.path(), &["scan"]);
    assert_eq!(res.code, 10);
}

// --- a usage error under --json still emits the machine-readable envelope ---
#[test]
fn usage_error_json_emits_error_envelope() {
    let dir = init_repo();
    let res = rep(dir.path(), &["badcmd", "--json"]);
    assert_eq!(res.code, 10);
    let json = res.json();
    assert_eq!(json["schema_version"], "rep.error.v1");
    assert_eq!(json["error"]["kind"], "invalid_arguments");
    assert_eq!(json["error"]["exit_code"], 10);
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

// --json failures emit a machine-readable error envelope
#[test]
fn json_failures_emit_error_envelope() {
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

// a hash mismatch at apply records state=failed
#[test]
fn hash_mismatch_marks_plan_failed() {
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

// a transient dirty tree leaves the plan in state=planned
#[test]
fn dirty_tree_stays_planned() {
    let dir = setup_success_example();
    let plan_id = plan_three_maps(dir.path(), &[]);
    write(dir.path(), "README.md", "# dirtied\n");
    let res = rep(dir.path(), &["apply", "--plan", &plan_id, "--json"]);
    assert_eq!(res.code, 4);
    // A transient dirty tree must not mark the plan failed.
    let status = rep(dir.path(), &["status", "--json"]);
    assert_eq!(status.json()["state"], "planned");
}

// a case-only rename is rejected with exit code 6
#[test]
fn case_only_rename_rejected() {
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

// a no-op plan returns exit code 2 and writes no artifacts
#[test]
fn no_op_plan_exit_2() {
    let dir = init_repo();
    write(dir.path(), "hello.txt", "hello world\n");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "init"]);
    let res = rep(dir.path(), &["plan", "--map", "absent=present", "--json"]);
    assert_eq!(res.code, 2);
    assert!(!dir.path().join(".rep/plans").exists());
}

// the content preview artifact is named content-preview.txt
#[test]
fn content_preview_artifact_name() {
    let dir = setup_success_example();
    let plan_id = plan_three_maps(dir.path(), &[]);
    let base = dir.path().join(format!(".rep/plans/{plan_id}"));
    assert!(base.join("content-preview.txt").exists());
    assert!(!base.join("content.patch").exists());
}

// distinct plans get distinct ids and directories
#[test]
fn plan_ids_are_unique() {
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

// a tracked symlink can be rename-planned without reading its target
#[cfg(unix)]
#[test]
fn symlink_rename_does_not_follow_target() {
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

// excluding .rep is allowed (only --include is rejected)
#[test]
fn exclude_rep_allowed() {
    let dir = setup_success_example();
    let res = rep(
        dir.path(),
        &["scan", "oldname", "--exclude", ".rep/**", "--json"],
    );
    assert_eq!(res.code, 0);
}

// --map-file alone drives a full plan/apply; comments and blank lines skipped
#[test]
fn map_file_plans_and_applies() {
    let dir = setup_success_example();
    write(
        dir.path(),
        "maps.txt",
        "# casing variants\noldname=newname\n\nOldName=NewName\nOLDNAME=NEWNAME\n",
    );
    git(dir.path(), &["add", "maps.txt"]);
    git(dir.path(), &["commit", "-q", "-m", "maps"]);
    let res = rep(
        dir.path(),
        &[
            "plan",
            "--map-file",
            "maps.txt",
            "--exclude",
            "maps.txt",
            "--json",
        ],
    );
    assert_eq!(res.code, 0, "plan failed: {}", res.stdout);
    let plan_id = res.json()["plan_id"].as_str().unwrap().to_string();
    let res = rep(dir.path(), &["apply", "--plan", &plan_id, "--json"]);
    assert_eq!(res.code, 0, "apply failed: {}", res.stdout);
    let content = read(dir.path(), "src/oldname.ts");
    assert!(content.contains("newname"));
    assert!(content.contains("NEWNAME"));
}

// --map and --map-file combine; plan.json records maps first, file entries after
#[test]
fn map_and_map_file_combine_in_plan_json() {
    let dir = setup_success_example();
    write(dir.path(), "maps.txt", "OldName=NewName\nOLDNAME=NEWNAME\n");
    let res = rep(
        dir.path(),
        &[
            "plan",
            "--map",
            "oldname=newname",
            "--map-file",
            "maps.txt",
            "--json",
        ],
    );
    assert_eq!(res.code, 0, "plan failed: {}", res.stdout);
    let plan_id = res.json()["plan_id"].as_str().unwrap().to_string();
    let plan: Value = serde_json::from_str(&read(
        dir.path(),
        &format!(".rep/plans/{plan_id}/plan.json"),
    ))
    .unwrap();
    let froms: Vec<&str> = plan["mappings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["from"].as_str().unwrap())
        .collect();
    assert_eq!(froms, vec!["oldname", "OldName", "OLDNAME"]);
}

// an invalid map-file line is a usage error naming the file and line
#[test]
fn map_file_invalid_line_exit_10() {
    let dir = setup_success_example();
    write(dir.path(), "maps.txt", "oldname=newname\nnoequals\n");
    let res = rep(dir.path(), &["plan", "--map-file", "maps.txt", "--json"]);
    assert_eq!(res.code, 10);
    let msg = res.json()["error"]["message"].as_str().unwrap().to_string();
    assert!(msg.contains("maps.txt"), "message: {msg}");
    assert!(msg.contains("line 2"), "message: {msg}");
}

// duplicate FROM between --map and --map-file hits the usual validation
#[test]
fn map_file_duplicate_from_exit_10() {
    let dir = setup_success_example();
    write(dir.path(), "maps.txt", "oldname=other\n");
    let res = rep(
        dir.path(),
        &[
            "plan",
            "--map",
            "oldname=newname",
            "--map-file",
            "maps.txt",
            "--json",
        ],
    );
    assert_eq!(res.code, 10);
    let msg = res.json()["error"]["message"].as_str().unwrap().to_string();
    assert!(msg.contains("duplicate mapping FROM"), "message: {msg}");
}

// a missing map file is a usage error (exit 10), not a generic IO failure
#[test]
fn map_file_missing_exit_10() {
    let dir = setup_success_example();
    let res = rep(dir.path(), &["plan", "--map-file", "nope.txt", "--json"]);
    assert_eq!(res.code, 10);
    let msg = res.json()["error"]["message"].as_str().unwrap().to_string();
    assert!(msg.contains("nope.txt"), "message: {msg}");
}

// --map-file - reads mappings from stdin
#[test]
fn map_file_stdin() {
    let dir = setup_success_example();
    let res = rep_with_stdin(
        dir.path(),
        &["plan", "--map-file", "-", "--json"],
        "oldname=newname\nOldName=NewName\nOLDNAME=NEWNAME\n",
    );
    assert_eq!(res.code, 0, "plan failed: {}", res.stdout);
    assert_eq!(res.json()["content"]["replacements"].as_i64().unwrap(), 3);
}

// stdin may back at most one --map-file
#[test]
fn map_file_stdin_twice_exit_10() {
    let dir = setup_success_example();
    let res = rep_with_stdin(
        dir.path(),
        &["plan", "--map-file", "-", "--map-file", "-", "--json"],
        "oldname=newname\n",
    );
    assert_eq!(res.code, 10);
}

// matched_directories reports the token-bearing directory prefix
#[test]
fn matched_directory_prefix() {
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
