//! CLI argument parsing with clap.

use clap::{Parser, Subcommand};

/// The on-ramp shown on `-h`: purpose is in `about`, this is the first command.
const QUICK_START: &str = "\
EXAMPLE  (rename old_name -> new_name across the repo):
  rep scan old_name                  # see where it appears
  rep plan --map old_name=new_name   # preview the change; prints a <plan-id>
  rep apply --plan <plan-id>         # apply that plan
  rep residual old_name              # confirm the old name is gone

See more with '--help'.";

/// The richer footer for `--help`: the worked flow plus the concepts a
/// first-time user needs (`mapping`, `plan`, `residual`) and the exit-code note.
const QUICK_START_LONG: &str = "\
EXAMPLE  (rename old_name -> new_name across the repo):
  rep scan old_name                  # see where it appears
  rep plan --map old_name=new_name   # preview the change; prints a <plan-id>
  rep apply --plan <plan-id>         # apply that plan
  rep residual old_name              # confirm the old name is gone

CONCEPTS:
  mapping     a literal FROM=TO pair, passed with --map. No regex and no
              automatic case handling -- map each casing explicitly
              (e.g. --map OldName=NewName --map OLDNAME=NEWNAME).
  plan        a previewed, saved set of edits, identified by a <plan-id>.
  residual    leftover occurrences of the old token after applying.

Exit codes are stable for scripts and agents (0 success, 2 no matches,
8 residual found, ...). Run 'rep <command> --help' for per-command detail.";

#[derive(Parser)]
#[command(name = "rep")]
#[command(about = "Safe, machine-readable rename & token migration for git repositories")]
#[command(long_about = "\
Safe, machine-readable rename & token migration for git repositories.

Renames a token everywhere it appears -- in file contents and in file/folder \
names -- as one checkable pass, instead of hand-editing each file or running a \
risky find-and-replace.

Safe by design: 'plan' previews every change before 'apply' touches a file, and \
'residual' confirms the old token is gone afterward. Operates only on \
git-tracked files; commit or stash other changes first.")]
#[command(version)]
#[command(after_help = QUICK_START)]
#[command(after_long_help = QUICK_START_LONG)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Emit machine-readable JSON (primary interface for AI coding agents)
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Find where a token appears in tracked file contents and paths
    #[command(long_about = "\
Find where a token appears in tracked file contents and paths.

This is the first step: it changes nothing, it just reports how many times the \
token occurs and in which files, so you can decide what to rename.")]
    #[command(after_help = "Example:\n  rep scan old_name")]
    Scan {
        /// The token to look for (literal text, e.g. old_name)
        token: String,

        /// Match case-insensitively (ASCII)
        #[arg(long)]
        case_insensitive: bool,

        /// Only consider paths matching these globs (repeatable)
        #[arg(long)]
        include: Vec<String>,

        /// Skip paths matching these globs (repeatable)
        #[arg(long)]
        exclude: Vec<String>,

        /// Restrict to git-tracked files (always on in the minimal version)
        #[arg(long)]
        tracked_only: bool,
    },

    /// Preview a rename and save it as a plan (does not change files)
    #[command(long_about = "\
Preview a rename and save it as a plan. This does NOT change any files.

You describe the rename as one or more literal mappings with --map FROM=TO \
(no regex; map each casing explicitly). rep computes every edit and prints a \
<plan-id>; pass that id to 'rep apply' to perform the change.")]
    #[command(
        after_help = "Example:\n  rep plan --map old_name=new_name --map OldName=NewName\n  # prints a <plan-id> -> next: rep apply --plan <plan-id>"
    )]
    Plan {
        /// A literal mapping FROM=TO, e.g. old_name=new_name (repeatable)
        #[arg(long = "map", value_name = "FROM=TO")]
        map: Vec<String>,

        /// Disable content replacement (enabled by default)
        #[arg(long = "no-content")]
        no_content: bool,

        /// Also rename matching tracked file and folder paths
        #[arg(long)]
        rename_paths: bool,

        /// Only consider paths matching these globs (repeatable)
        #[arg(long)]
        include: Vec<String>,

        /// Skip paths matching these globs (repeatable)
        #[arg(long)]
        exclude: Vec<String>,

        /// Restrict to git-tracked files (always on in the minimal version)
        #[arg(long)]
        tracked_only: bool,
    },

    /// Apply a previously previewed plan to the working tree
    #[command(long_about = "\
Apply a previously previewed plan to the working tree.

<PLAN> is the <plan-id> printed by 'rep plan'. apply refuses to run if tracked \
files changed since the plan was built, so the preview always matches what gets \
written.")]
    #[command(after_help = "Example:\n  rep apply --plan <plan-id>")]
    Apply {
        /// The <plan-id> printed by `rep plan`
        #[arg(long)]
        plan: String,
    },

    /// Confirm an old token is gone (leftover check after applying)
    #[command(long_about = "\
Confirm an old token is gone -- a leftover check you run after applying.

'residual' is any remaining occurrence of the old token in tracked content or \
paths. Pass the token directly, or use --plan to derive the tokens from a \
plan's FROM sides. Exits non-zero (code 8) if anything is left.")]
    #[command(after_help = "Example:\n  rep residual old_name\n  rep residual --plan <plan-id>")]
    Residual {
        /// The token to check is gone (omit when using --plan)
        token: Option<String>,

        /// Derive tokens to check from a plan's mapping FROM sides
        #[arg(long)]
        plan: Option<String>,

        /// Match case-insensitively (ASCII)
        #[arg(long)]
        case_insensitive: bool,

        /// Only consider paths matching these globs (repeatable)
        #[arg(long)]
        include: Vec<String>,

        /// Skip paths matching these globs (repeatable)
        #[arg(long)]
        exclude: Vec<String>,

        /// Restrict to git-tracked files (always on in the minimal version)
        #[arg(long)]
        tracked_only: bool,
    },

    /// Show rep's current state for this repository (last plan, etc.)
    Status,
}
