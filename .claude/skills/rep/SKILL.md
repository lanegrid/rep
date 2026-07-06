---
name: rep
description: Safely rename or migrate a token/name across a git repository using the rep CLI (scan -> plan -> apply -> residual -> status). Use when the user wants a repo-wide rename of an identifier, package name, namespace, env var, or directory across tracked files, instead of ad-hoc sed/scripts.
argument-hint: <old-token> [new-token]
allowed-tools: Bash(rep*), Bash(git*), Read, Grep
---

# rep — safe repo-wide rename / token migration

`rep` performs mechanical renames as an auditable pipeline over **git-tracked
files only**, with stable JSON output. Always pass `--json` and branch on the
exit code.

```text
scan -> plan -> apply -> residual -> status
```

## Before you start

- Be inside a git repository (`rep` exits `3` otherwise).
- **Ensure `.rep/` is gitignored.** rep writes its plan artifacts there —
  machine-managed state that must never be committed, and as an untracked
  directory it pollutes `git status` and trips tools that require a clean
  tree. Check and fix before the first `rep plan`:

  ```bash
  git check-ignore -q .rep || echo '.rep/' >> .gitignore
  ```

- `apply` requires a **clean tracked tree** (commit or stash first; exit `4` if
  dirty). Untracked files are never touched.
- Mappings are **explicit and literal** — no regex, no auto case handling. Add
  one `--map FROM=TO` per case variant.

## Workflow

### 1. Scan — measure and discover case variants

```bash
rep scan <old-token> --case-insensitive --json
```

Read `content.variants` (e.g. `oldname`, `OldName`, `OLDNAME`) and
`paths.matched_*`. The variants tell you exactly which mappings to write.

### 2. Build the mappings

One mapping per variant found, mapping each to its corresponding new form:

```bash
rep plan \
  --map oldname=newname \
  --map OldName=NewName \
  --map OLDNAME=NEWNAME \
  --rename-paths \
  --json
```

- Add `--rename-paths` only if `scan` reported path matches you want renamed.
- Omit it to change file **contents** only.
- `plan` never modifies the working tree; it writes artifacts under
  `.rep/plans/<plan_id>/` and prints the `plan_id`.

If `plan` exits `2`, the mappings matched nothing — re-check the tokens.
If it exits `6`, there is a path conflict (target exists, two sources collide,
or a case-only rename) — adjust the mappings or resolve the target first.

**Many mappings?** Do not build a giant `--map` argument list. Write one
`FROM=TO` per line (blank lines and `#` comments allowed) and pass the file —
or pipe via stdin with `-`:

```bash
rep plan --map-file mappings.txt --json
```

**Files already renamed with `git mv`?** Derive the mappings from the staged
renames instead of reconstructing them by hand:

```bash
git mv src/old_name.ts src/new_name.ts       # stage renames first
rep plan --from-git-renames --json           # derives old_name=new_name
```

Only same-directory, same-extension stem changes are derivable; everything
else (e.g. a file-to-dir move like `x.ts -> x/scene.ts`) is listed under
`derived.underivable` with a reason — cover those with explicit `--map`
entries. Note the staged renames leave the tree dirty, so this plan cannot be
applied yet: record the derived mappings (they are echoed in `derived` and in
the plan), commit the renames, then re-plan from those mappings and apply.

### 3. Review (optional)

```bash
rep show --json                    # most recent plan: mappings, counts, state
rep show --files --skipped --json  # what it touches, and what was skipped why
rep show --preview                 # line-level content preview
```

`rep show` is the supported way to inspect a plan — do not read
`.rep/plans/<plan_id>/*.json` directly.

### 4. Apply

```bash
rep apply --last --json            # the plan you just made (rep status shows it)
rep apply --plan <plan_id> --json  # or an explicit plan id
```

Guarded by repo/HEAD match, clean-tree, and per-file hash checks. On failure the
exit code says why: `4` dirty tree (commit/stash, retry), `5` stale plan (HEAD
moved — re-plan), `6` path conflict, `7` file changed since plan (re-plan).

### 5. Residual — prove the old token is gone

```bash
rep residual <old-token> --case-insensitive --json
# or, against every mapping FROM in the plan you just applied:
rep residual --last --json
rep residual --plan <plan_id> --json
```

`passed: true` (exit `0`) means no occurrences in tracked content or paths.
Exit `8` means residual found — inspect `content.files` / `paths.files`.

### 6. Status

```bash
rep status --json
```

Reports `none` / `planned` / `applied` / `failed` plus the active plan and a
suggested next command.

## Exit codes

```text
0 success            5 stale plan          9 apply failed
1 general error      6 path conflict       10 invalid arguments
2 no matches         7 file hash mismatch
3 not a git repo     8 residual found
4 tracked tree dirty
```

## Recovery

`rep` has no `undo`. Because it only touches tracked files on a clean tree,
revert with git:

```bash
git reset --hard HEAD   # discard an applied change
```

## Notes

- `.rep/` is always excluded and cannot be re-included.
- Scope with `--include GLOB` / `--exclude GLOB` (repeatable).
- Excludes that hold for *every* rename in a repo (lockfiles, generated
  artifacts) belong in a checked-in `rep.toml` at the repo root, not on each
  command line:

  ```toml
  [scope]
  exclude = ["pnpm-lock.yaml", "projects/*/publish.json"]
  ```

  It applies to `scan` / `plan` / `residual`; CLI globs are additive on top;
  `--no-config` skips it for one run. Config-skipped paths are reported with
  reason `excluded_by_config`.
- For agents: `--json` is the contract — both success and failure print JSON;
  the exit code drives control flow.
