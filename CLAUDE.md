# rep

`rep` is a CLI that lets AI coding agents and developers run repository-wide
renames / token migrations safely. It treats a mechanical change as an explicit,
machine-readable pipeline:

```text
scan -> plan -> apply -> residual -> status
```

It operates only on git-tracked files, performs explicit literal mappings (no
regex, no automatic case handling), and emits stable JSON for every command.

## Bash / task execution convention

This repository's execution entry point is `mise run <task>`. Every bash tool
call must follow these rules.

1. **Route every bash tool call through `mise run <task>`.** Do not invoke
   `cargo` / `pnpm` / `python` / `node` / `tsc` / `uv run` etc. directly. All
   execution is consolidated into tasks so runs are reproducible and reviewable.
2. **Name tasks `<namespace>:<verb>`.** `<namespace>` is the full package name
   (`rep` — never abbreviated), `<verb>` is a verb (nestable with `:`). The only
   sanctioned non-package namespaces are the cross-cutting umbrellas `repo:*`,
   `git:*`, `studio:*`. The canonical task list lives in
   `docs/operations/tasks.md`.
3. **Raw bash is allowed only when unavoidable, and only read-only.** Observation
   commands (`ls`, `cat`, `grep`, `git status`, `git log`, …) may be raw. Raw
   bash that writes or has side effects is forbidden. The write exceptions are
   the operations owned by a managed flow: `commit` / `push` / `gh pr create`
   (the git-workflow skill — follow `/git-workflow`) and the version bump / tag /
   `gh release create` of a release (the release skill — follow `/release`).
4. **Do not make output lossy.** Do not suppress with `2>/dev/null` / `|| true`,
   and do not trim with `tail` / `head` / pipes. Read the full output of a
   `mise run` (including errors) and judge from it.
5. **If a needed command can't run via `mise run`, add a task — don't fall back
   to raw bash.** Define it in `tasks.toml` (`packages/<pkg>/tasks.toml`, or the
   top-level `tasks.toml`/`mise.toml` for cross-cutting work) and document its
   trigger / prerequisites in the owning skill (or `docs/operations/tasks.md`).
   "Just this once, raw" is debt.
6. **One bash tool call = one purpose.** No decorative separators like
   `echo "==="`. Do not chain independent observations with `;` / `&&` into a
   single call (it makes it ambiguous which output belongs to which command and
   buries failures). Split independent observations into separate tool calls —
   run them in parallel when there is no dependency.

## Codebase content convention

Keep the codebase self-contained. Do not embed references that only resolve
inside a transient context the code itself does not carry (a review thread, a
chat, an external tracker). To a future reader they are dead pointers.

This applies to code, comments, test names, and artifacts. Forbidden:

- Identifiers from a review or conversation used as labels.
- A ticket, PR, or issue number used as the only explanation, without the actual
  reason stated inline.
- A document section number used in place of describing the behavior.

Instead, name things by the behavior or rule they encode, and let comments
explain why the code behaves as it does. Durable references to files that live
in this repo (paths, module names) are fine.

## Development

This project uses [mise](https://mise.jdx.dev/) for task running. Tasks are
defined in `mise.toml` and documented in `docs/operations/tasks.md`.

| Command | Description |
|---------|-------------|
| `mise run rep:verify` | Run all checks (fmt, check, lint, test, build) |
| `mise run rep:fmt` | Check formatting |
| `mise run rep:fmt:fix` | Fix formatting |
| `mise run rep:lint` | Run clippy lints |
| `mise run rep:lint:fix` | Fix clippy lints |
| `mise run rep:test` | Run tests |
| `mise run rep:build` | Build debug binary |
| `mise run rep:build:release` | Build release binary |
| `mise run rep:install` | Build and install `rep` locally (dogfooding) |
| `mise run rep:smoke` | Run the full pipeline against a throwaway temp repo (dev check) |

### Before committing

Always run `mise run rep:verify` before committing to ensure formatting, clippy,
tests, and the build all pass.

### Releasing

Use the `/release` skill to cut a new version:

```
/release <version>
```

It bumps `Cargo.toml`, runs `mise run rep:verify`, commits `chore: release
vX.X.X`, tags `vX.X.X`, and creates the GitHub release. Pushing the tag triggers
`.github/workflows/release.yml`, which builds multi-target binaries and attaches
them (plus `install.sh`) to the release. CI (`.github/workflows/ci.yml`) runs
test / fmt / clippy / build / lockfile on every push and PR to `main`.

## Project structure

```text
src/
├── main.rs         # Entry point: parse CLI, dispatch, map errors to exit codes
├── lib.rs          # Library root
├── cli.rs          # clap argument parsing
├── error.rs        # Error type + fixed exit codes
├── schema.rs       # Stable schema_version identifiers
├── output/         # Styled human output + JSON printing
├── git/            # Git abstraction (query + mutation)
├── globset.rs      # Minimal include/exclude glob matching
├── text.rs         # Literal mappings, replacement, occurrence counting
├── scope.rs        # Scope resolution + file gathering + hashing
├── path_rename.rs  # File-level rename planning + conflict detection
├── scanner.rs      # `rep scan`
├── planner.rs      # `rep plan`
├── applier.rs      # `rep apply`
├── residual.rs     # `rep residual`
├── status.rs       # `rep status`
└── artifacts.rs    # `.rep/` plan model + read/write + state pointer
tests/
└── cli.rs          # End-to-end CLI tests (acceptance criteria)
```

## Exit codes

```text
0  success            5  stale plan          9  apply failed
1  general error      6  path conflict       10 invalid arguments
2  no matches         7  file hash mismatch
3  not a git repo     8  residual found
4  tracked tree dirty
```
