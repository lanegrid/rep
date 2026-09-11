# rep

`rep` is a CLI for running repository-wide renames and token migrations
**safely**, with **machine-readable** output. It is built for AI coding agents
(and developers) who would otherwise reach for `sed` or ad-hoc scripts and lose
track of what changed.

Instead of a blind string replace, `rep` models a rename as an explicit
pipeline:

```text
scan -> plan -> apply -> residual -> status
```

## Principles

- **Tracked files only.** Operates strictly on `git ls-files`; untracked,
  ignored, and out-of-repo files are never touched. Side effects stay inside the
  git working tree, where they can be inspected or discarded.
- **Explicit literal mappings.** No regex, no automatic case handling. Every
  case variant is its own `--map FROM=TO`.
- **`plan` never mutates.** It writes artifacts under `.rep/plans/<id>/`; the
  working tree is left untouched.
- **`apply` is guarded.** It requires a clean tracked tree, a matching `HEAD`,
  and matching per-file hashes before writing anything.
- **Stable JSON.** Every command supports `--json` — the primary interface for
  agents.

## Commands

```sh
rep scan TOKEN [--case-insensitive] [--include GLOB] [--exclude GLOB] [--json]
rep plan --map FROM=TO [--map ...] [--rename-paths] [--no-content] [--json]
rep apply --plan PLAN_ID [--json]
rep residual TOKEN [--case-insensitive] [--json]
rep residual --plan PLAN_ID [--json]
rep status [--json]
```

## Example

Given `src/oldname.ts`:

```ts
export const OLDNAME_DATA_ROOT = "~/Movies/oldname"
export class OldNameClient {}
```

```sh
rep scan oldname --case-insensitive --json

rep plan \
  --map oldname=newname \
  --map OldName=NewName \
  --map OLDNAME=NEWNAME \
  --rename-paths \
  --json

rep apply --plan <PLAN_ID> --json

rep residual oldname --case-insensitive --json   # passed: true
rep status --json                                 # state: applied
```

Result: `src/oldname.ts` is `git mv`-ed to `src/newname.ts`, the three case
variants are rewritten in its content, and no `oldname` remains in tracked
content or paths. Untracked files and anything outside the repo are left alone.

## Exit codes

```text
0  success            5  stale plan          9  apply failed
1  general error      6  path conflict       10 invalid arguments
2  no matches         7  file hash mismatch
3  not a git repo     8  residual found
4  tracked tree dirty
```

With `--json`, both success **and** failure print JSON: the exit code controls
flow, the JSON explains it. Failures emit a `rep.error.v1` envelope on stdout:

```json
{
  "schema_version": "rep.error.v1",
  "error": { "kind": "tracked_tree_dirty", "message": "...", "exit_code": 4 }
}
```

A `plan` that would change nothing exits `2` and writes no new plan artifacts.

`apply --last` is blocked (exit `5`, `stale_plan`) when the latest `plan`
attempt failed, found no changes, or was interrupted, including argument and
map-file errors. A successful new `plan` enables it again. Help requests do not
invalidate it. `status --json` exposes `apply_last_blocked`; older plans remain
available through `show --plan ID` and `residual --plan ID`. After reviewing an
older plan, you can deliberately use `apply --plan ID`; all normal validation
still applies. This guard is stored under `.rep/`, so retain that directory
between commands. Run plan/apply commands sequentially within a repository.

## Planning a larger migration

rep performs literal changes; it does not infer what a string means. A
repository path such as `projects/<id>` and an external recording destination
with the same text may need different treatment, even within one file.
`--include`/`--exclude` scope entire files, not individual occurrences. Inspect
`rep show --preview` before applying; use a separate semantic edit when file
scope and literal mappings cannot express the intended distinction.

For a directory move, use `plan --map OLD=NEW --rename-paths --no-content`
to plan only tracked path changes. Moving a file does not recalculate relative
imports, and `--from-git-renames` derives literal mappings rather than resolving
imports. Repair location-dependent imports with language-aware tooling and run
the project's type checks, tests, and build.

Ignored and untracked files are excluded from both migration and residual
checks. Inventory them separately (`git ls-files --others --ignored
--exclude-standard` and `git ls-files --others --exclude-standard`); reconcile
them with an explicit collision policy before removing old directories.
A passing `residual` check proves absence only within its tracked-file scope,
not correctness of imports, external paths, or the whole migration.

In scripts, stop on any nonzero plan exit and apply the exact `plan_id` returned
by that successful invocation. rep's benefit is its saved preview, validation,
and audit artifacts; no speed advantage is claimed without measurements.

## Installation

```bash
# Quick install (macOS/Linux)
curl -fsSL https://github.com/lanegrid/rep/releases/latest/download/install.sh | sh

# From source
git clone https://github.com/lanegrid/rep.git
cd rep
cargo install --path .
```

Pre-built binaries are attached to each [GitHub release](https://github.com/lanegrid/rep/releases) for:

- macOS (Intel & Apple Silicon)
- Linux (x64 & ARM64)
- Windows (x64)

## Development

This repository uses [mise](https://mise.jdx.dev/) for task running; see
[`docs/operations/tasks.md`](docs/operations/tasks.md) and
[`CLAUDE.md`](CLAUDE.md).

```sh
mise run rep:verify   # fmt + check + lint + test + build
```

## License

MIT OR Apache-2.0
