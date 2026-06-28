# Tasks

This is the canonical source of truth for runnable tasks in this repository.
Every side-effecting command runs through `mise run <task>` (see the execution
convention in `CLAUDE.md`).

## Naming

Tasks are named `<namespace>:<verb>`:

- `<namespace>` is the full package name (`rep`), or a sanctioned cross-cutting
  umbrella namespace: `repo:*`, `git:*`, `studio:*`.
- `<verb>` is a verb, optionally nested with `:` (e.g. `fmt:fix`).

## `rep` package tasks

| Task | Description |
|------|-------------|
| `mise run rep:verify` | Run all checks (fmt, check, lint, test, build) |
| `mise run rep:check` | `cargo check` across all targets |
| `mise run rep:fmt` | Check formatting |
| `mise run rep:fmt:fix` | Apply formatting |
| `mise run rep:lint` | Run clippy with warnings denied |
| `mise run rep:lint:fix` | Auto-fix clippy lints where possible |
| `mise run rep:test` | Run the test suite |
| `mise run rep:build` | Build the debug binary |
| `mise run rep:build:release` | Build the release binary |
| `mise run rep:install` | Build and install `rep` locally (dogfooding) |

## Releasing

Releases are not a mise task — use the `/release` skill (`/release <version>`).
It bumps the version, runs `mise run rep:verify`, commits, tags `vX.X.X`, and
creates the GitHub release; pushing the tag triggers the binary-build workflow.

## Adding a task

When you need a command that no task covers, add a task rather than running raw
bash:

1. Define it in `mise.toml` (top-level here, since this is a single-package
   repo; in a monorepo use `packages/<pkg>/tasks.toml`).
2. Document its trigger / prerequisites here (or in the owning skill).
