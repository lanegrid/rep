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
  mapping     a literal FROM=TO pair, passed with --map (or one per line in a
              file via --map-file). No regex and no automatic case handling --
              map each casing explicitly
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
(no regex; map each casing explicitly), or list many in a file passed with \
--map-file. rep computes every edit and prints a <plan-id>; pass that id to \
'rep apply' to perform the change.")]
    #[command(
        after_help = "Example:\n  rep plan --map old_name=new_name --map OldName=NewName\n  rep plan --map-file mappings.txt   # one FROM=TO per line; '-' reads stdin\n  # prints a <plan-id> -> next: rep apply --plan <plan-id>"
    )]
    Plan {
        /// A literal mapping FROM=TO, e.g. old_name=new_name (repeatable)
        #[arg(long = "map", value_name = "FROM=TO")]
        map: Vec<String>,

        /// Read mappings from a file, one FROM=TO per line; blank lines and
        /// '#' comments are skipped; '-' reads stdin (repeatable)
        #[arg(long = "map-file", value_name = "PATH")]
        map_file: Vec<String>,

        /// Derive FROM=TO mappings from renames staged in the git index
        /// (e.g. after `git mv old.ts new.ts`)
        #[arg(
            long = "from-git-renames",
            long_help = "Derive FROM=TO mappings from renames staged in the git index (e.g. after \
`git mv old.ts new.ts`; unstaged renames cannot be detected by git).

Only a rename that keeps its directory and extension but changes the file stem \
is derivable; every other staged rename is reported as 'underivable' with a \
reason, so you can describe it with explicit --map entries. Derived mappings \
merge with --map/--map-file and are recorded in the plan output under 'derived'.

Note that staged renames leave the tracked tree dirty, so the plan cannot be \
applied until they are committed -- and committing moves HEAD, which staleness \
checks reject. The working recipe: stage renames, run \
'rep plan --from-git-renames --json' to derive and record the mappings, commit \
the renames, then re-plan from the recorded mappings and apply."
        )]
        from_git_renames: bool,

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

<PLAN> is the <plan-id> printed by 'rep plan'; '--last' applies the most \
recent plan (the one 'rep status' shows) without copying the id. apply \
refuses to run if tracked files changed since the plan was built, so the \
preview always matches what gets written.")]
    #[command(
        after_help = "Example:\n  rep apply --plan <plan-id>\n  rep apply --last               # the plan `rep status` shows"
    )]
    #[command(group = clap::ArgGroup::new("plan_ref").required(true).args(["plan", "last"]))]
    Apply {
        /// The <plan-id> printed by `rep plan`
        #[arg(long)]
        plan: Option<String>,

        /// Apply the most recent plan (the one `rep status` shows)
        #[arg(long)]
        last: bool,
    },

    /// Confirm an old token is gone (leftover check after applying)
    #[command(long_about = "\
Confirm an old token is gone -- a leftover check you run after applying.

'residual' is any remaining occurrence of the old token in tracked content or \
paths. Pass the token directly, or use --plan (or --last for the most recent \
plan) to derive the tokens from a plan's FROM sides. Exits non-zero (code 8) \
if anything is left.")]
    #[command(
        after_help = "Example:\n  rep residual old_name\n  rep residual --plan <plan-id>\n  rep residual --last"
    )]
    Residual {
        /// The token to check is gone (omit when using --plan/--last)
        token: Option<String>,

        /// Derive tokens to check from a plan's mapping FROM sides
        #[arg(long)]
        plan: Option<String>,

        /// Derive tokens from the most recent plan (the one `rep status` shows)
        #[arg(long, conflicts_with_all = ["plan", "token"])]
        last: bool,

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

    /// Inspect a saved plan (summary; add --files/--skipped/--preview detail)
    #[command(long_about = "\
Inspect a saved plan without reading .rep/plans/<plan-id>/ files by hand.

Prints the plan's mappings, counts, state, and suggested next command. \
--files, --skipped, and --preview add the corresponding detail sections. \
With no --plan, shows the most recent plan (the one 'rep status' shows).")]
    #[command(
        after_help = "Example:\n  rep show                           # most recent plan\n  rep show --plan <plan-id> --skipped"
    )]
    Show {
        /// The <plan-id> to inspect (defaults to the most recent plan)
        #[arg(long, conflicts_with = "last")]
        plan: Option<String>,

        /// Inspect the most recent plan (the default; accepted for symmetry
        /// with `rep apply --last`)
        #[arg(long)]
        last: bool,

        /// Also list planned content files and path renames
        #[arg(long)]
        files: bool,

        /// Also list skipped paths with reasons
        #[arg(long)]
        skipped: bool,

        /// Also print the line-level content preview
        #[arg(long)]
        preview: bool,
    },

    /// Show rep's current state for this repository (last plan, etc.)
    Status,
}
