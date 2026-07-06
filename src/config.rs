//! `rep.toml` — checked-in scope defaults for scan / plan / residual.
//!
//! Lives at the repository root (not inside `.rep/`, which is the
//! machine-managed plan store and always excluded from scope) so the policy
//! is tracked and reviewed like any other project convention.

use std::path::Path;

use serde::Deserialize;

use crate::error::{RepError, Result};

/// File name of the scope config at the repository root.
pub const CONFIG_FILE: &str = "rep.toml";

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub scope: ScopeConfig,
}

/// The `[scope]` table: globs applied before any CLI `--include`/`--exclude`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeConfig {
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// Load `rep.toml` from the repository root; `Ok(None)` when absent.
///
/// Parse failures — including unknown keys, which are almost always typos of
/// `include`/`exclude` — are usage errors (exit 10), not generic IO errors,
/// so a broken config never silently widens the scope.
pub fn load(root: &Path) -> Result<Option<Config>> {
    let path = root.join(CONFIG_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(&path)?;
    let config: Config = toml::from_str(&data)
        .map_err(|e| RepError::InvalidArguments(format!("invalid {CONFIG_FILE}: {e}")))?;
    Ok(Some(config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_table_parses() {
        let config: Config =
            toml::from_str("[scope]\nexclude = [\"vendor/**\"]\ninclude = [\"src/**\"]\n").unwrap();
        assert_eq!(config.scope.exclude, vec!["vendor/**"]);
        assert_eq!(config.scope.include, vec!["src/**"]);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        // `exlude` is the typo this guard exists for.
        assert!(toml::from_str::<Config>("[scope]\nexlude = [\"x\"]\n").is_err());
        assert!(toml::from_str::<Config>("[scop]\n").is_err());
    }

    #[test]
    fn empty_file_is_a_valid_empty_config() {
        let config: Config = toml::from_str("").unwrap();
        assert!(config.scope.exclude.is_empty());
        assert!(config.scope.include.is_empty());
    }
}
