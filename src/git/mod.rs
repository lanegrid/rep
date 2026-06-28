//! Git abstraction layer.
//!
//! All operations run with their working directory pinned to the repository
//! root so that paths returned by `git ls-files` are always relative to that
//! root, regardless of the process's current directory.

pub mod mutation;
pub mod query;

pub use mutation::*;
pub use query::*;
