//! `deploy-from` branch patterns. An entry is an exact branch name, `*` for any
//! branch, or a glob in which `*` matches any characters within one `/`
//! segment and `?` matches one character, so `*.x` matches `26.6.x` and
//! `release/*` matches `release/2026` but not `release/a/b`.

/// Whether `branch` matches any of `patterns`.
#[must_use]
pub fn allows(patterns: &[String], branch: &str) -> bool {
    patterns.iter().any(|pattern| matches(pattern, branch))
}

/// Whether `branch` matches `pattern`.
#[must_use]
pub fn matches(pattern: &str, branch: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let pattern = pattern.chars().collect::<Vec<_>>();
    let branch = branch.chars().collect::<Vec<_>>();
    glob(&pattern, &branch)
}

/// Explains why `pattern` cannot be used, if it cannot.
pub fn validate(pattern: &str) -> Result<(), String> {
    if pattern.is_empty() {
        return Err("an empty entry".to_owned());
    }
    if let Some(character) = pattern
        .chars()
        .find(|character| character.is_whitespace() || character.is_control())
    {
        return Err(format!("'{pattern}' contains {character:?}"));
    }
    if let Some(character) = pattern
        .chars()
        .find(|character| matches!(character, '~' | '^' | ':' | '\\' | '[' | ']'))
    {
        return Err(format!(
            "'{pattern}' contains '{character}', which is not allowed in a branch pattern"
        ));
    }
    if pattern.contains("**") {
        return Err(format!(
            "'{pattern}' uses '**'; '*' matches within one '/' segment, so list each depth"
        ));
    }
    Ok(())
}

fn glob(pattern: &[char], branch: &[char]) -> bool {
    match pattern.split_first() {
        None => branch.is_empty(),
        Some(('*', rest)) => {
            let mut skipped = 0;
            loop {
                if glob(rest, branch.get(skipped..).unwrap_or_default()) {
                    return true;
                }
                match branch.get(skipped) {
                    Some('/') | None => return false,
                    Some(_) => skipped += 1,
                }
            }
        }
        Some(('?', rest)) => {
            matches!(branch.split_first(), Some((character, tail)) if *character != '/' && glob(rest, tail))
        }
        Some((expected, rest)) => {
            matches!(branch.split_first(), Some((character, tail)) if character == expected && glob(rest, tail))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_names_and_any_branch() {
        assert!(matches("main", "main"));
        assert!(!matches("main", "main2"));
        assert!(!matches("main", "feature/main"));
        assert!(matches("*", "feature/a/b"));
    }

    #[test]
    fn globs_stay_within_one_segment() {
        assert!(matches("*.x", "26.6.x"));
        assert!(!matches("*.x", "26.6.1"));
        assert!(!matches("*.x", "hotfix/26.6.x"));
        assert!(matches("release/*", "release/2026"));
        assert!(!matches("release/*", "release/a/b"));
        assert!(!matches("release/*", "release"));
        assert!(matches("26.?.x", "26.6.x"));
        assert!(!matches("26.?.x", "26.10.x"));
        assert!(matches("*/*", "a/b"));
    }

    #[test]
    fn allows_any_listed_pattern() {
        let patterns = vec!["main".to_owned(), "*.x".to_owned()];
        assert!(allows(&patterns, "main"));
        assert!(allows(&patterns, "26.6.x"));
        assert!(!allows(&patterns, "feature/login"));
    }

    #[test]
    fn rejects_patterns_git_or_the_matcher_cannot_use() {
        assert!(validate("main").is_ok());
        assert!(validate("*.x").is_ok());
        assert!(validate("*").is_ok());
        assert!(validate("").is_err());
        assert!(validate(" main").is_err());
        assert!(validate("release/**").is_err());
        assert!(validate("v[0-9]").is_err());
        assert!(validate("a:b").is_err());
    }
}
