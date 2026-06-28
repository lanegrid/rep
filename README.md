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

## Development

This repository uses [mise](https://mise.jdx.dev/) for task running; see
[`docs/operations/tasks.md`](docs/operations/tasks.md) and
[`CLAUDE.md`](CLAUDE.md).

```sh
mise run rep:verify   # fmt + check + lint + test + build
```

## License

MIT OR Apache-2.0
