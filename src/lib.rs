//! rep - safe, machine-readable rename & token migration for git repositories
//!
//! `rep` treats repository-wide mechanical renames as an explicit pipeline:
//! `scan -> plan -> apply -> residual -> status`. Every step operates only on
//! git-tracked files and emits stable JSON for AI coding agents to consume.

pub mod applier;
pub mod artifacts;
pub mod cli;
pub mod error;
pub mod git;
pub mod globset;
pub mod output;
pub mod path_rename;
pub mod planner;
pub mod residual;
pub mod scanner;
pub mod schema;
pub mod scope;
pub mod status;
pub mod text;

pub use error::{RepError, Result};
