---
name: release
description: Release a new version of rep
argument-hint: <version>
allowed-tools: Bash(mise*), Bash(git*), Bash(gh*), Read, Edit, Grep
---

# rep Release

Release version `$ARGUMENTS`.

## Steps

First, gather current state:
- Run `grep '^version' Cargo.toml` to check the current version
- Run `git describe --tags --abbrev=0` to get the latest tag (may be empty for the first release)
- Run `git log <latest-tag>..HEAD --oneline` to see commits since the last release

1. **Validate version**
   - Verify the version format (e.g., 0.1.1)
   - Ensure the tag `vX.X.X` does not already exist
   - Ensure you are on `main` with a clean tree, synced with `origin/main`

2. **Update Cargo.toml**
   - Update `version = "X.X.X"` to the new version

3. **Quality checks**
   ```bash
   mise run rep:verify
   ```
   This runs fmt, check, lint, test, and build.

4. **Commit**
   - Message: `chore: release vX.X.X`

5. **Generate release notes**
   Analyze commits since the last release and produce notes in this format:

   ```markdown
   ## What's Changed

   ### Features
   - Commits starting with feat:

   ### Bug Fixes
   - Commits starting with fix:

   ### Other Changes
   - Other commits (chore, docs, refactor, etc.)

   **Full Changelog**: https://github.com/lanegrid/rep/compare/v{prev}...v{new}
   ```

6. **Create tag and push**
   ```bash
   git tag vX.X.X
   git push origin main
   git push origin vX.X.X
   ```

7. **Create GitHub release**
   ```bash
   gh release create vX.X.X --title "vX.X.X" --notes "release notes here"
   ```

## Notes

- Pushing the `vX.X.X` tag triggers the Release workflow, which builds
  multi-target binaries and attaches them (plus `install.sh`) to the GitHub
  release created in step 7.
- The release flow does not publish to crates.io.
- Highlight breaking changes if any. Keep release notes concise but informative.
