//! Literal token mapping and content replacement.
//!
//! `rep` only performs explicit literal mappings (no regex, no automatic
//! case-preservation). Mappings are applied in the order they are given.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{RepError, Result};

/// A single literal mapping `from -> to`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Mapping {
    pub from: String,
    pub to: String,
}

/// Parse a `FROM=TO` argument into a [`Mapping`].
///
/// The split happens at the first `=`, so `TO` may itself contain `=`.
pub fn parse_mapping(spec: &str) -> Result<Mapping> {
    let (from, to) = spec.split_once('=').ok_or_else(|| {
        RepError::InvalidArguments(format!("invalid --map '{spec}', expected FROM=TO"))
    })?;
    if from.is_empty() {
        return Err(RepError::InvalidArguments(format!(
            "invalid --map '{spec}', FROM must not be empty"
        )));
    }
    Ok(Mapping {
        from: from.to_string(),
        to: to.to_string(),
    })
}

/// Validate that a set of mappings can be applied unambiguously.
///
/// Fails when mappings are duplicated or interfere with one another (e.g.
/// `foo=bar` together with `bar=baz`, or one `FROM` being a substring of
/// another), since the result would depend on application order.
pub fn validate_mappings(maps: &[Mapping]) -> Result<()> {
    if maps.is_empty() {
        return Err(RepError::InvalidArguments(
            "at least one --map FROM=TO is required".to_string(),
        ));
    }
    for (i, a) in maps.iter().enumerate() {
        for (j, b) in maps.iter().enumerate() {
            if i == j {
                continue;
            }
            if a.from == b.from {
                return Err(RepError::InvalidArguments(format!(
                    "duplicate mapping FROM '{}'",
                    a.from
                )));
            }
            if a.to.contains(&b.from) {
                return Err(RepError::InvalidArguments(format!(
                    "ambiguous mappings: '{}' -> '{}' interferes with FROM '{}'",
                    a.from, a.to, b.from
                )));
            }
            if a.from.contains(&b.from) {
                return Err(RepError::InvalidArguments(format!(
                    "ambiguous mappings: FROM '{}' contains FROM '{}'",
                    a.from, b.from
                )));
            }
        }
    }
    Ok(())
}

/// Apply mappings to `content`, returning the new content and the number of
/// replacements performed. Mappings are guaranteed non-interfering by
/// [`validate_mappings`], so sequential application is order-stable.
pub fn apply(content: &str, maps: &[Mapping]) -> (String, usize) {
    let mut out = content.to_string();
    let mut count = 0;
    for m in maps {
        if m.from.is_empty() {
            continue;
        }
        let occ = out.matches(&m.from).count();
        if occ > 0 {
            out = out.replace(&m.from, &m.to);
            count += occ;
        }
    }
    (out, count)
}

/// Find the byte offsets of every occurrence of `token` in `text`.
///
/// Case-insensitive matching is ASCII-only, which keeps byte offsets aligned
/// with the original text (sufficient for identifier-like tokens).
pub fn find_all(text: &str, token: &str, case_insensitive: bool) -> Vec<usize> {
    if token.is_empty() {
        return Vec::new();
    }
    let (hay, needle) = if case_insensitive {
        (text.to_ascii_lowercase(), token.to_ascii_lowercase())
    } else {
        (text.to_string(), token.to_string())
    };
    let hb = hay.as_bytes();
    let nb = needle.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + nb.len() <= hb.len() {
        if &hb[i..i + nb.len()] == nb {
            out.push(i);
            i += nb.len();
        } else {
            i += 1;
        }
    }
    out
}

/// Count occurrences of `token` in `text`.
pub fn count(text: &str, token: &str, case_insensitive: bool) -> usize {
    find_all(text, token, case_insensitive).len()
}

/// Tally the surface forms (case variants) of `token` found in `text`.
pub fn variants(text: &str, token: &str, case_insensitive: bool) -> BTreeMap<String, usize> {
    let mut map = BTreeMap::new();
    let len = token.len();
    for off in find_all(text, token, case_insensitive) {
        let surface = text[off..off + len].to_string();
        *map.entry(surface).or_insert(0) += 1;
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(from: &str, to: &str) -> Mapping {
        Mapping {
            from: from.to_string(),
            to: to.to_string(),
        }
    }

    #[test]
    fn parse_splits_on_first_equals() {
        let parsed = parse_mapping("FOO=a=b").unwrap();
        assert_eq!(parsed.from, "FOO");
        assert_eq!(parsed.to, "a=b");
    }

    #[test]
    fn parse_rejects_empty_from() {
        assert!(parse_mapping("=bar").is_err());
        assert!(parse_mapping("noequals").is_err());
    }

    #[test]
    fn case_variants_are_independent_mappings() {
        let maps = vec![
            m("oldname", "newname"),
            m("OldName", "NewName"),
            m("OLDNAME", "NEWNAME"),
        ];
        validate_mappings(&maps).unwrap();
        let (out, n) = apply("oldname OldName OLDNAME", &maps);
        assert_eq!(out, "newname NewName NEWNAME");
        assert_eq!(n, 3);
    }

    #[test]
    fn interfering_mappings_rejected() {
        assert!(validate_mappings(&[m("foo", "bar"), m("bar", "baz")]).is_err());
        assert!(validate_mappings(&[m("ab", "x"), m("abc", "y")]).is_err());
        assert!(validate_mappings(&[m("foo", "x"), m("foo", "y")]).is_err());
    }

    #[test]
    fn empty_mappings_rejected() {
        assert!(validate_mappings(&[]).is_err());
    }

    #[test]
    fn count_and_variants() {
        let text = "oldname OldName oldname OLDNAME";
        assert_eq!(count(text, "oldname", false), 2);
        assert_eq!(count(text, "oldname", true), 4);
        let v = variants(text, "oldname", true);
        assert_eq!(v.get("oldname"), Some(&2));
        assert_eq!(v.get("OldName"), Some(&1));
        assert_eq!(v.get("OLDNAME"), Some(&1));
    }

    #[test]
    fn newlines_preserved() {
        let (out, _) = apply("a\noldname\r\nb", &[m("oldname", "newname")]);
        assert_eq!(out, "a\nnewname\r\nb");
    }
}
