//! Styled console output utilities.
//!
//! Human-readable output is secondary; the `--json` form is the primary
//! interface for AI coding agents.

use serde::Serialize;

use crate::error::Result;

/// ANSI color codes
const GREEN: &str = "\x1b[0;32m";
const YELLOW: &str = "\x1b[0;33m";
const BLUE: &str = "\x1b[0;34m";
const RED: &str = "\x1b[0;31m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

/// Output helper for informational messages (ℹ)
pub fn info(msg: &str) {
    println!("{BLUE}ℹ{RESET} {msg}");
}

/// Output helper for success messages (✓)
pub fn success(msg: &str) {
    println!("{GREEN}✓{RESET} {msg}");
}

/// Output helper for warning messages (⚠)
pub fn warn(msg: &str) {
    println!("{YELLOW}⚠{RESET} {msg}");
}

/// Output helper for error messages (✗) — written to stderr.
pub fn error(msg: &str) {
    eprintln!("{RED}✗{RESET} {msg}");
}

/// Output helper for action/suggestion messages (→)
pub fn action(msg: &str) {
    println!("{BOLD}→{RESET} {msg}");
}

/// Format text as bold.
pub fn bold(text: &str) -> String {
    format!("{BOLD}{text}{RESET}")
}

/// Print a serializable value as pretty JSON to stdout.
pub fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
