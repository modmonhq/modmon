use regex::Regex;

/// The patterns of a `.dockerignore` file, matched the way BuildKit matches
/// them when it sends a build context.
#[derive(Debug, Clone, Default)]
pub struct ContextIgnore {
    patterns: Vec<IgnorePattern>,
    exception_prefixes: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
struct IgnorePattern {
    pattern: String,
    exception: bool,
    form: PatternForm,
}

#[derive(Debug, Clone)]
enum PatternForm {
    Exact,
    Prefix,
    Suffix,
    Expression(Regex),
}

/// Whether a path is excluded, and the patterns it matched, which its
/// children inherit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IgnoreVerdict {
    excluded: bool,
    matched: Vec<bool>,
}

impl IgnoreVerdict {
    #[must_use]
    pub const fn excluded(&self) -> bool {
        self.excluded
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidIgnoreLine {
    pub line: usize,
    pub pattern: String,
}

impl std::fmt::Display for InvalidIgnoreLine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "line {}: `{}` is not a valid pattern",
            self.line, self.pattern
        )
    }
}

impl std::error::Error for InvalidIgnoreLine {}

impl ContextIgnore {
    pub fn parse(text: &str) -> Result<Self, InvalidIgnoreLine> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut patterns = Vec::new();
        for (index, line) in text.lines().enumerate() {
            if line.starts_with('#') {
                continue;
            }
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let invalid = || InvalidIgnoreLine {
                line: index + 1,
                pattern: line.to_owned(),
            };
            let (exception, pattern) = match line.strip_prefix('!') {
                Some(rest) => (true, rest.trim()),
                None => (false, line),
            };
            if pattern.is_empty() {
                return Err(invalid());
            }
            let mut pattern = clean(pattern);
            if pattern.len() > 1 && pattern.starts_with('/') {
                pattern.remove(0);
            }
            let form = compile(&pattern).ok_or_else(invalid)?;
            patterns.push(IgnorePattern {
                pattern,
                exception,
                form,
            });
        }
        let wildcard_exception = patterns.iter().any(|pattern| {
            pattern.exception && without_trailing_glob(&pattern.pattern).contains(WILDCARDS)
        });
        let exception_prefixes = (!wildcard_exception).then(|| {
            patterns
                .iter()
                .filter(|pattern| pattern.exception)
                .map(|pattern| format!("{}/", without_trailing_glob(&pattern.pattern)))
                .collect()
        });
        Ok(Self {
            patterns,
            exception_prefixes,
        })
    }

    /// The verdict for `path`, a context-relative path with `/` separators,
    /// given the verdict of the directory that holds it (`None` at the top
    /// of the context).
    #[must_use]
    pub fn verdict(&self, path: &str, parent: Option<&IgnoreVerdict>) -> IgnoreVerdict {
        let inherited = parent.map_or(&[][..], |parent| parent.matched.as_slice());
        let mut excluded = false;
        let mut matched = vec![false; self.patterns.len()];
        for (index, pattern) in self.patterns.iter().enumerate() {
            let mut hit = inherited.get(index).copied().unwrap_or(false);
            if !hit {
                if pattern.exception != excluded {
                    continue;
                }
                hit = pattern.matches(path)
                    || (inherited.is_empty()
                        && path
                            .match_indices('/')
                            .any(|(end, _)| pattern.matches(&path[..end])));
            }
            matched[index] = hit;
            if hit {
                excluded = !pattern.exception;
            }
        }
        IgnoreVerdict { excluded, matched }
    }

    /// Whether nothing below an excluded directory can be sent, so the
    /// directory need not be read.
    #[must_use]
    pub fn skips_excluded_directory(&self, path: &str) -> bool {
        self.exception_prefixes.as_ref().is_some_and(|prefixes| {
            let directory = format!("{path}/");
            !prefixes
                .iter()
                .any(|prefix| prefix.starts_with(directory.as_str()))
        })
    }
}

const WILDCARDS: &[char] = &['*', '[', ']', '?', '^', '\\'];

impl IgnorePattern {
    fn matches(&self, path: &str) -> bool {
        match &self.form {
            PatternForm::Exact => path == self.pattern,
            PatternForm::Prefix => path.starts_with(&self.pattern[..self.pattern.len() - 2]),
            PatternForm::Suffix => {
                let suffix = &self.pattern[2..];
                path.ends_with(suffix) || suffix.strip_prefix('/') == Some(path)
            }
            PatternForm::Expression(expression) => expression.is_match(path),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    Exact,
    Prefix,
    Suffix,
    Expression,
}

fn compile(pattern: &str) -> Option<PatternForm> {
    let mut expression = String::from("^");
    let mut form = Form::Exact;
    let mut characters = pattern.chars().peekable();
    let mut first = true;
    while let Some(character) = characters.next() {
        match character {
            '*' if characters.peek() == Some(&'*') => {
                characters.next();
                if characters.peek() == Some(&'/') {
                    characters.next();
                }
                if characters.peek().is_none() {
                    if form == Form::Exact {
                        form = Form::Prefix;
                    } else {
                        expression.push_str(".*");
                        form = Form::Expression;
                    }
                } else {
                    expression.push_str("(.*/)?");
                    form = Form::Expression;
                }
                if first {
                    form = Form::Suffix;
                }
            }
            '*' => {
                expression.push_str("[^/]*");
                form = Form::Expression;
            }
            '?' => {
                expression.push_str("[^/]");
                form = Form::Expression;
            }
            '.' | '+' | '(' | ')' | '|' | '{' | '}' | '$' => {
                expression.push('\\');
                expression.push(character);
            }
            '\\' => {
                let escaped = characters.next()?;
                expression.push_str(&regex::escape(escaped.encode_utf8(&mut [0; 4])));
                form = Form::Expression;
            }
            '[' | ']' => {
                expression.push(character);
                form = Form::Expression;
            }
            other => expression.push(other),
        }
        first = false;
    }
    Some(match form {
        Form::Exact => PatternForm::Exact,
        Form::Prefix => PatternForm::Prefix,
        Form::Suffix => PatternForm::Suffix,
        Form::Expression => {
            expression.push('$');
            PatternForm::Expression(Regex::new(&expression).ok()?)
        }
    })
}

fn clean(path: &str) -> String {
    let rooted = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|last| *last != "..") => {
                parts.pop();
            }
            ".." if rooted => {}
            other => parts.push(other),
        }
    }
    let joined = parts.join("/");
    if rooted {
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".to_owned()
    } else {
        joined
    }
}

fn without_trailing_glob(pattern: &str) -> &str {
    let pattern = pattern.strip_suffix("/**").unwrap_or(pattern);
    pattern.strip_suffix("/*").unwrap_or(pattern)
}
