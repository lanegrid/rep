//! Custom error types and fixed exit codes for the rep CLI.
//!
//! Exit codes are stable so AI coding agents can branch on them:
//!
//! ```text
//! 0  success
//! 1  general error
//! 2  no matches
//! 3  not a git repository
//! 4  tracked tree is dirty
//! 5  stale plan
//! 6  path conflict
//! 7  file hash mismatch
//! 8  residual found
//! 9  apply failed
//! 10 invalid arguments
//! ```

use serde::Serialize;
use thiserror::Error;

use crate::schema;

/// Result type alias for rep operations.
pub type Result<T> = std::result::Result<T, RepError>;

/// Custom error type for the rep CLI.
#[derive(Error, Debug)]
pub enum RepError {
    #[error("{0}")]
    General(String),

    #[error("not a git repository")]
    NotAGitRepository,

    #[error("tracked tree is dirty; commit or stash changes first")]
    TrackedTreeDirty,

    #[error("stale plan: {0}")]
    StalePlan(String),

    #[error("path conflict: {0}")]
    PathConflict(String),

    #[error("file hash mismatch: {0}")]
    FileHashMismatch(String),

    #[error("apply failed: {0}")]
    ApplyFailed(String),

    #[error("invalid arguments: {0}")]
    InvalidArguments(String),

    #[error("git command failed: {0}")]
    Git(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl RepError {
    /// Map an error to its fixed CLI exit code.
    pub fn exit_code(&self) -> i32 {
        match self {
            RepError::General(_) => 1,
            RepError::NotAGitRepository => 3,
            RepError::TrackedTreeDirty => 4,
            RepError::StalePlan(_) => 5,
            RepError::PathConflict(_) => 6,
            RepError::FileHashMismatch(_) => 7,
            RepError::ApplyFailed(_) => 9,
            RepError::InvalidArguments(_) => 10,
            RepError::Git(_) => 1,
            RepError::Io(_) => 1,
            RepError::Json(_) => 1,
        }
    }

    /// A concrete next step for human output, shown under the error message.
    ///
    /// Errors are read most often right after a failure, so where the fix is a
    /// specific command we name it. The JSON envelope deliberately omits this
    /// (agents branch on `exit_code`/`kind`, not prose).
    pub fn hint(&self) -> Option<&'static str> {
        match self {
            RepError::NotAGitRepository => {
                Some("Run rep inside a git repository, or `git init` here first.")
            }
            RepError::TrackedTreeDirty => {
                Some("Commit or stash your other changes, then re-run the command.")
            }
            RepError::StalePlan(_) => {
                Some("Rebuild it with `rep plan ...`, then `rep apply --plan <plan-id>`.")
            }
            _ => None,
        }
    }

    /// Stable, machine-readable kind for the JSON error output.
    pub fn kind(&self) -> &'static str {
        match self {
            RepError::General(_) => "general_error",
            RepError::NotAGitRepository => "not_a_git_repository",
            RepError::TrackedTreeDirty => "tracked_tree_dirty",
            RepError::StalePlan(_) => "stale_plan",
            RepError::PathConflict(_) => "path_conflict",
            RepError::FileHashMismatch(_) => "file_hash_mismatch",
            RepError::ApplyFailed(_) => "apply_failed",
            RepError::InvalidArguments(_) => "invalid_arguments",
            RepError::Git(_) => "git_error",
            RepError::Io(_) => "io_error",
            RepError::Json(_) => "json_error",
        }
    }

    /// Build the machine-readable error envelope for `--json` failures.
    pub fn to_output(&self) -> ErrorOutput {
        ErrorOutput {
            schema_version: schema::ERROR.to_string(),
            error: ErrorBody {
                kind: self.kind().to_string(),
                message: self.to_string(),
                exit_code: self.exit_code(),
            },
        }
    }
}

/// The `error` field of [`ErrorOutput`].
#[derive(Serialize)]
pub struct ErrorBody {
    pub kind: String,
    pub message: String,
    pub exit_code: i32,
}

/// Machine-readable error envelope (`rep.error.v1`).
#[derive(Serialize)]
pub struct ErrorOutput {
    pub schema_version: String,
    pub error: ErrorBody,
}
