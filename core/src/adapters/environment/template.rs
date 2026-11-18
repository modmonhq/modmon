use std::collections::BTreeMap;
use std::path::Path;

use crate::adapters::config::{is_global_key, is_variable_name};
use crate::domain::diagnostic::Diagnostic;
use crate::domain::environment::{SourceLocation, VariableDeclaration, is_safe_environment_value};

#[derive(Debug, Clone, Default)]
pub struct ParsedTemplate {
    pub variables: Vec<VariableDeclaration>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TemplateParser;

impl TemplateParser {
    #[must_use]
    pub fn parse(&self, path: &Path, source: &str) -> ParsedTemplate {
        let mut parsed = ParsedTemplate::default();
        let mut declarations = BTreeMap::<String, usize>::new();

        if source.contains('\0') {
            parsed.diagnostics.push(
                Diagnostic::error(
                    "MM_ENV_TEMPLATE_NUL",
                    "environment templates may not contain NUL bytes",
                )
                .at_path(path),
            );
            return parsed;
        }

        for (line_index, raw_line) in source.lines().enumerate() {
            let line_number = line_index + 1;
            match parse_template_line(path, line_number, raw_line, &mut declarations) {
                Ok(Some(variable)) => parsed.variables.push(variable),
                Ok(None) => {}
                Err(diagnostic) => parsed.diagnostics.push(*diagnostic),
            }
        }

        parsed
    }
}

fn parse_template_line(
    path: &Path,
    line_number: usize,
    raw_line: &str,
    declarations: &mut BTreeMap<String, usize>,
) -> Result<Option<VariableDeclaration>, Box<Diagnostic>> {
    let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(None);
    }
    let declaration = trimmed
        .strip_prefix("export ")
        .map_or(trimmed, str::trim_start);
    let (raw_name, remainder) = declaration.split_once('=').ok_or_else(|| {
        Box::new(template_error(
            path,
            line_number,
            1,
            "MM_ENV_TEMPLATE_ASSIGNMENT",
            "expected KEY=VALUE assignment",
        ))
    })?;
    let name = raw_name.trim();
    let column = line.find(name).map_or(1, |index| index + 1);
    if !is_variable_name(name) {
        return Err(Box::new(template_error(
            path,
            line_number,
            column,
            "MM_ENV_VARIABLE_INVALID",
            format!("'{name}' is not a valid environment variable name"),
        )));
    }
    if let Some(previous_line) = declarations.insert(name.to_owned(), line_number) {
        return Err(Box::new(
            template_error(
                path,
                line_number,
                column,
                "MM_ENV_VARIABLE_DUPLICATE",
                format!("environment variable '{name}' is declared more than once"),
            )
            .with_help(format!("first declaration is on line {previous_line}")),
        ));
    }
    let (value_source, comment) = split_comment(remainder).map_err(|message| {
        Box::new(template_error(
            path,
            line_number,
            column,
            "MM_ENV_TEMPLATE_VALUE",
            message,
        ))
    })?;
    let (value, explicit_value) = parse_value(value_source).map_err(|message| {
        Box::new(template_error(
            path,
            line_number,
            column,
            "MM_ENV_TEMPLATE_VALUE",
            message,
        ))
    })?;
    if !is_safe_environment_value(&value) {
        return Err(Box::new(template_error(
            path,
            line_number,
            column,
            "MM_ENV_TEMPLATE_VALUE",
            "environment values may not contain control characters",
        )));
    }
    build_variable(ParsedVariable {
        path,
        line: line_number,
        column,
        name,
        value,
        explicit_value,
        comment,
    })
    .map(Some)
}

struct ParsedVariable<'a> {
    path: &'a Path,
    line: usize,
    column: usize,
    name: &'a str,
    value: String,
    explicit_value: bool,
    comment: Option<&'a str>,
}

fn build_variable(input: ParsedVariable<'_>) -> Result<VariableDeclaration, Box<Diagnostic>> {
    let ParsedVariable {
        path,
        line,
        column,
        name,
        value,
        explicit_value,
        comment,
    } = input;
    let directive = comment.and_then(parse_directive);
    let (global_key, required, error) = match directive {
        None => (None, false, None),
        Some(Ok(directive)) => (directive.global_key, directive.required, directive.error),
        Some(Err(message)) => (None, false, Some(message)),
    };
    if let Some(message) = error {
        return Err(Box::new(template_error(
            path,
            line,
            column,
            "MM_ENV_TEMPLATE_DIRECTIVE",
            message,
        )));
    }
    if required && global_key.is_none() {
        return Err(Box::new(template_error(
            path,
            line,
            column,
            "MM_ENV_TEMPLATE_DIRECTIVE",
            "required may only be used with mm:from=GLOBAL.KEY",
        )));
    }
    if let Some(global_key) = &global_key
        && !is_global_key(global_key)
    {
        return Err(Box::new(template_error(
            path,
            line,
            column,
            "MM_ENV_GLOBAL_INVALID",
            format!("'{global_key}' is not a valid global environment key"),
        )));
    }
    let has_binding = global_key.is_some();
    Ok(VariableDeclaration {
        name: name.to_owned(),
        global_key,
        required,
        default_value: has_binding
            .then(|| explicit_value.then_some(value.clone()))
            .flatten(),
        literal_value: (!has_binding).then_some(value),
        location: SourceLocation {
            path: path.to_path_buf(),
            line,
            column,
        },
    })
}

#[derive(Debug)]
struct Directive {
    global_key: Option<String>,
    required: bool,
    error: Option<String>,
}

fn parse_directive(comment: &str) -> Option<Result<Directive, String>> {
    let content = comment.trim_start_matches('#').trim_start();
    let content = content.strip_prefix("mm:")?;
    let mut global_key = None;
    let mut required = false;
    let mut error = None;

    for token in content.split_whitespace() {
        if let Some(key) = token.strip_prefix("from=") {
            if global_key.replace(key.to_owned()).is_some() {
                error = Some("directive contains more than one from= binding".to_owned());
            }
        } else if token == "required" {
            if required {
                error = Some("directive contains required more than once".to_owned());
            }
            required = true;
        } else {
            return Some(Err(format!("unknown mm directive token '{token}'")));
        }
    }
    if global_key.is_none() {
        error = Some("mm directive requires from=GLOBAL.KEY".to_owned());
    }

    Some(Ok(Directive {
        global_key,
        required,
        error,
    }))
}

fn split_comment(value: &str) -> Result<(&str, Option<&str>), String> {
    let mut quote = None;
    let mut escaped = false;
    let mut previous = None;

    for (index, character) in value.char_indices() {
        if escaped {
            escaped = false;
            previous = Some(character);
            continue;
        }
        if character == '\\' && quote == Some('"') {
            escaped = true;
            previous = Some(character);
            continue;
        }
        if matches!(character, '\'' | '"') {
            if quote == Some(character) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(character);
            }
            previous = Some(character);
            continue;
        }
        if character == '#' && quote.is_none() && previous.is_none_or(char::is_whitespace) {
            return Ok((&value[..index], Some(&value[index..])));
        }
        previous = Some(character);
    }

    if quote.is_some() || escaped {
        Err("unterminated quoted value".to_owned())
    } else {
        Ok((value, None))
    }
}

fn parse_value(value: &str) -> Result<(String, bool), String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok((String::new(), false));
    }
    if let Some(inner) = value.strip_prefix('\'') {
        let Some(inner) = inner.strip_suffix('\'') else {
            return Err("unterminated single-quoted value".to_owned());
        };
        if inner.contains('\'') {
            return Err("unescaped quote inside single-quoted value".to_owned());
        }
        return Ok((inner.to_owned(), true));
    }
    if let Some(inner) = value.strip_prefix('"') {
        let Some(inner) = inner.strip_suffix('"') else {
            return Err("unterminated double-quoted value".to_owned());
        };
        return decode_double_quoted(inner).map(|decoded| (decoded, true));
    }
    if value.contains(['\'', '"']) {
        return Err("quotes must delimit the entire environment value".to_owned());
    }
    Ok((value.to_owned(), true))
}

fn decode_double_quoted(value: &str) -> Result<String, String> {
    let mut decoded = String::new();
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character == '"' {
            return Err("unescaped quote inside double-quoted value".to_owned());
        }
        if character != '\\' {
            decoded.push(character);
            continue;
        }
        let escaped = characters
            .next()
            .ok_or_else(|| "value ends with an incomplete escape".to_owned())?;
        decoded.push(match escaped {
            '\\' => '\\',
            '"' => '"',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            other => return Err(format!("unsupported escape \\{other}")),
        });
    }
    Ok(decoded)
}

fn template_error(
    path: &Path,
    line: usize,
    column: usize,
    code: &str,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic::error(code, message).at_location(path, line, column)
}

#[cfg(test)]
#[path = "template/tests.rs"]
mod tests;
