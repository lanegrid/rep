//! CLI argument parsing with clap.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "rep")]
#[command(about = "Safe, machine-readable rename & token migration for git repositories")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Emit machine-readable JSON (primary interface for AI coding agents)
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Measure how a token appears in tracked content and paths
    Scan {
        /// The token to scan for
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

    /// Build a change plan from explicit literal mappings
    Plan {
        /// A literal mapping FROM=TO (repeatable)
        #[arg(long = "map", value_name = "FROM=TO")]
        map: Vec<String>,

        /// Disable content replacement (enabled by default)
        #[arg(long = "no-content")]
        no_content: bool,

        /// Also rename tracked file paths
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

    /// Apply a previously created plan
    Apply {
        /// The plan id to apply
        #[arg(long)]
        plan: String,
    },

    /// Confirm an old token no longer appears in tracked content or paths
    Residual {
        /// The token to check (omit when using --plan)
        token: Option<String>,

        /// Derive tokens from a plan's mapping FROM sides
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

    /// Report the current rep state for the repository
    Status,
}
