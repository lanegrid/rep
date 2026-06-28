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

use thiserror::Error;

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
}
