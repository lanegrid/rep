//! Stable `schema_version` identifiers for every JSON output.
//!
//! Bump these when an output shape changes in a backwards-incompatible way.

/// `rep scan` output schema version.
pub const SCAN: &str = "rep.scan.v1";
/// `rep plan` artifact / output schema version.
pub const PLAN: &str = "rep.plan.v1";
/// `rep apply` output schema version.
pub const APPLY: &str = "rep.apply.v1";
/// `rep residual` output schema version.
pub const RESIDUAL: &str = "rep.residual.v1";
/// `rep status` output schema version.
pub const STATUS: &str = "rep.status.v1";

/// The directory `rep` writes its artifacts to (always excluded from scope).
pub const REP_DIR: &str = ".rep";
/// Glob that always excludes the rep artifact directory.
pub const DEFAULT_EXCLUDE: &str = ".rep/**";
