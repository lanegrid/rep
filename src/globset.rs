//! Minimal glob matching for `--include` / `--exclude` rules.
//!
//! Supported syntax:
//!
//! ```text
//! ?    a single character that is not '/'
//! *    zero or more characters that are not '/'
//! **   zero or more characters including '/'
//! ```
//!
//! Everything else matches literally. Paths use '/' as the separator (the form
//! produced by `git ls-files`).

/// Returns true if `path` matches the glob `pattern`.
pub fn glob_match(pattern: &str, path: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = path.chars().collect();
    match_from(&p, 0, &t, 0)
}

/// Returns the first glob in `globs` that matches `path`, if any.
pub fn first_match<'a>(globs: &'a [String], path: &str) -> Option<&'a str> {
    globs
        .iter()
        .find(|g| glob_match(g, path))
        .map(|s| s.as_str())
}

/// Returns true if any glob in `globs` matches `path`.
pub fn any_match(globs: &[String], path: &str) -> bool {
    globs.iter().any(|g| glob_match(g, path))
}

fn match_from(p: &[char], pi: usize, t: &[char], ti: usize) -> bool {
    let mut pi = pi;
    let mut ti = ti;
    while pi < p.len() {
        match p[pi] {
            '*' => {
                if pi + 1 < p.len() && p[pi + 1] == '*' {
                    // '**' matches across separators.
                    let mut next = pi + 2;
                    if next < p.len() && p[next] == '/' {
                        next += 1;
                    }
                    if next >= p.len() {
                        return true;
                    }
                    let mut k = ti;
                    loop {
                        if match_from(p, next, t, k) {
                            return true;
                        }
                        if k >= t.len() {
                            return false;
                        }
                        k += 1;
                    }
                } else {
                    // Single '*' matches within a path segment only.
                    let next = pi + 1;
                    let mut k = ti;
                    loop {
                        if match_from(p, next, t, k) {
                            return true;
                        }
                        if k >= t.len() || t[k] == '/' {
                            return false;
                        }
                        k += 1;
                    }
                }
            }
            '?' => {
                if ti >= t.len() || t[ti] == '/' {
                    return false;
                }
                pi += 1;
                ti += 1;
            }
            c => {
                if ti >= t.len() || t[ti] != c {
                    return false;
                }
                pi += 1;
                ti += 1;
            }
        }
    }
    ti == t.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_match() {
        assert!(glob_match("src/main.rs", "src/main.rs"));
        assert!(!glob_match("src/main.rs", "src/lib.rs"));
    }

    #[test]
    fn single_star_stays_in_segment() {
        assert!(glob_match("src/*.rs", "src/main.rs"));
        assert!(!glob_match("src/*.rs", "src/inner/main.rs"));
        assert!(glob_match("*.rs", "main.rs"));
        assert!(!glob_match("*.rs", "src/main.rs"));
    }

    #[test]
    fn double_star_crosses_segments() {
        assert!(glob_match("dist/**", "dist/bundle.js"));
        assert!(glob_match("dist/**", "dist/a/b/c.js"));
        assert!(glob_match(".rep/**", ".rep/plans/x/plan.json"));
        assert!(!glob_match(".rep/**", "src/.rep.txt"));
    }

    #[test]
    fn double_star_prefix() {
        assert!(glob_match("**/foo.rs", "a/b/foo.rs"));
        assert!(glob_match("**/foo.rs", "foo.rs"));
        assert!(!glob_match("**/foo.rs", "a/b/bar.rs"));
    }

    #[test]
    fn question_mark() {
        assert!(glob_match("a?c", "abc"));
        assert!(!glob_match("a?c", "a/c"));
    }

    #[test]
    fn first_match_reports_rule() {
        let globs = vec!["dist/**".to_string(), "*.log".to_string()];
        assert_eq!(first_match(&globs, "dist/bundle.js"), Some("dist/**"));
        assert_eq!(first_match(&globs, "debug.log"), Some("*.log"));
        assert_eq!(first_match(&globs, "src/main.rs"), None);
    }
}
