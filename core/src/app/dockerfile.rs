use std::collections::BTreeSet;
use std::ops::Range;

use crate::domain::build::{BuildDependencyReference, ResolvedBuildDependency};
use crate::domain::error::MmError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DockerfileBaseKind {
    Literal { repository: String },
    Variable { name: String },
    Stage,
    Scratch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DockerfileBase {
    pub kind: DockerfileBaseKind,
    pub original_reference: String,
    pub instruction_index: u32,
    pub source_line: u32,
    pub stage: Option<String>,
    reference_range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedDockerfile {
    pub bases: Vec<DockerfileBase>,
    pub declared_args: BTreeSet<String>,
}

struct LogicalInstruction {
    text: String,
    source_offsets: Vec<usize>,
    source_line: u32,
}

pub(crate) fn parse(source: &str, label: &str) -> Result<ParsedDockerfile, MmError> {
    let instructions = logical_instructions(source, label)?;
    let mut bases = Vec::new();
    let mut declared_args = BTreeSet::new();
    let mut stages = BTreeSet::new();

    for (instruction_index, instruction) in instructions.iter().enumerate() {
        let tokens = tokens(&instruction.text);
        let Some((name, _, _)) = tokens.first() else {
            continue;
        };
        if instruction.text.contains("<<") {
            return Err(parse_error(
                label,
                instruction.source_line,
                "Dockerfile heredoc syntax is not supported by deterministic build planning",
            ));
        }
        if name.eq_ignore_ascii_case("ARG") {
            parse_arg(&tokens, label, instruction.source_line, &mut declared_args)?;
            continue;
        }
        if !name.eq_ignore_ascii_case("FROM") {
            continue;
        }
        let index = u32::try_from(instruction_index).map_err(|_| {
            parse_error(
                label,
                instruction.source_line,
                "Dockerfile contains too many instructions",
            )
        })?;
        let base = parse_from(instruction, &tokens, index, label, &stages)?;
        if let Some(stage) = &base.stage
            && !stages.insert(stage.to_ascii_lowercase())
        {
            return Err(parse_error(
                label,
                base.source_line,
                &format!("Dockerfile stage alias '{stage}' is duplicated"),
            ));
        }
        bases.push(base);
    }

    Ok(ParsedDockerfile {
        bases,
        declared_args,
    })
}

pub(crate) fn rewrite_dependencies(
    source: &str,
    label: &str,
    dependencies: &[ResolvedBuildDependency],
) -> Result<String, MmError> {
    let parsed = parse(source, label)?;
    let mut replacements = Vec::new();
    for dependency in dependencies {
        let BuildDependencyReference::Dockerfile {
            original_reference,
            instruction_index,
            source_line,
        } = &dependency.edge.reference
        else {
            continue;
        };
        let base = parsed
            .bases
            .iter()
            .find(|base| base.instruction_index == *instruction_index)
            .ok_or_else(|| {
                parse_error(
                    label,
                    *source_line,
                    "reviewed first-party FROM instruction is missing from the snapshot",
                )
            })?;
        if base.original_reference != *original_reference || base.source_line != *source_line {
            return Err(parse_error(
                label,
                *source_line,
                "reviewed first-party FROM instruction changed before execution",
            ));
        }
        replacements.push((
            base.reference_range.clone(),
            &dependency.immutable_reference,
        ));
    }
    replacements.sort_by_key(|replacement| std::cmp::Reverse(replacement.0.start));
    let mut rewritten = source.to_owned();
    for (range, replacement) in replacements {
        rewritten.replace_range(range, replacement);
    }
    Ok(rewritten)
}

fn logical_instructions(source: &str, label: &str) -> Result<Vec<LogicalInstruction>, MmError> {
    let mut instructions = Vec::new();
    let mut text = String::new();
    let mut source_offsets = Vec::new();
    let mut first_line = 1_u32;
    let mut continuing = false;
    let mut offset = 0_usize;

    for (line_index, raw_line) in source.split_inclusive('\n').enumerate() {
        let line_number = u32::try_from(line_index + 1).map_err(|_| {
            parse_error(label, u32::MAX, "Dockerfile contains too many source lines")
        })?;
        let line = raw_line
            .strip_suffix('\n')
            .unwrap_or(raw_line)
            .strip_suffix('\r')
            .unwrap_or_else(|| raw_line.strip_suffix('\n').unwrap_or(raw_line));
        let trimmed_start = line.trim_start();
        if trimmed_start.starts_with('#') {
            if !continuing
                && let Some(directive) = trimmed_start.get(1..).map(str::trim)
                && let Some((name, escape)) = directive.split_once('=')
                && name.eq_ignore_ascii_case("escape")
                && escape.trim() != "\\"
            {
                return Err(parse_error(
                    label,
                    line_number,
                    "only the default backslash Dockerfile escape character is supported",
                ));
            }
            offset += raw_line.len();
            continue;
        }
        if trimmed_start.is_empty() {
            if continuing {
                return Err(parse_error(
                    label,
                    line_number,
                    "blank lines inside continued Dockerfile instructions are unsupported",
                ));
            }
            offset += raw_line.len();
            continue;
        }
        if !continuing {
            first_line = line_number;
        } else {
            text.push(' ');
            source_offsets.push(offset);
        }

        let content_end = line.trim_end().len();
        let is_continued = line[..content_end].ends_with('\\');
        let append_end = if is_continued {
            content_end.saturating_sub(1)
        } else {
            line.len()
        };
        for (relative, character) in line[..append_end].char_indices() {
            text.push(character);
            source_offsets.extend(std::iter::repeat_n(offset + relative, character.len_utf8()));
        }
        continuing = is_continued;
        if !continuing {
            instructions.push(LogicalInstruction {
                text: std::mem::take(&mut text),
                source_offsets: std::mem::take(&mut source_offsets),
                source_line: first_line,
            });
        }
        offset += raw_line.len();
    }
    if continuing {
        return Err(parse_error(
            label,
            first_line,
            "Dockerfile ends during a continued instruction",
        ));
    }
    Ok(instructions)
}

fn parse_arg(
    tokens: &[(&str, usize, usize)],
    label: &str,
    source_line: u32,
    declared: &mut BTreeSet<String>,
) -> Result<(), MmError> {
    if tokens.len() != 2 {
        return Err(parse_error(
            label,
            source_line,
            "ARG must contain exactly one variable declaration",
        ));
    }
    let name = tokens[1]
        .0
        .split_once('=')
        .map_or(tokens[1].0, |(name, _)| name);
    if !is_variable_name(name) {
        return Err(parse_error(
            label,
            source_line,
            &format!("ARG '{name}' is not a supported variable name"),
        ));
    }
    declared.insert(name.to_owned());
    Ok(())
}

fn parse_from(
    instruction: &LogicalInstruction,
    tokens: &[(&str, usize, usize)],
    instruction_index: u32,
    label: &str,
    stages: &BTreeSet<String>,
) -> Result<DockerfileBase, MmError> {
    let mut position = 1;
    let mut platform_seen = false;
    while tokens
        .get(position)
        .is_some_and(|token| token.0.starts_with("--"))
    {
        let flag = tokens[position].0;
        if !flag.starts_with("--platform=") || flag.len() == "--platform=".len() || platform_seen {
            return Err(parse_error(
                label,
                instruction.source_line,
                &format!("unsupported FROM flag '{flag}'; only --platform=<value> is supported"),
            ));
        }
        platform_seen = true;
        position += 1;
    }
    let Some((reference, start, end)) = tokens.get(position).copied() else {
        return Err(parse_error(
            label,
            instruction.source_line,
            "FROM has no image reference",
        ));
    };
    position += 1;
    let stage = if position == tokens.len() {
        None
    } else if position + 2 == tokens.len() && tokens[position].0.eq_ignore_ascii_case("AS") {
        let alias = tokens[position + 1].0;
        if !is_stage_name(alias) {
            return Err(parse_error(
                label,
                instruction.source_line,
                &format!("FROM stage alias '{alias}' is invalid"),
            ));
        }
        Some(alias.to_owned())
    } else {
        return Err(parse_error(
            label,
            instruction.source_line,
            "FROM must use 'FROM [--platform=<value>] image [AS stage]'",
        ));
    };
    if !reference.is_ascii() {
        return Err(parse_error(
            label,
            instruction.source_line,
            "FROM image references must be ASCII",
        ));
    }
    let range_start = *instruction.source_offsets.get(start).ok_or_else(|| {
        parse_error(
            label,
            instruction.source_line,
            "could not locate FROM image reference",
        )
    })?;
    let range_end = instruction
        .source_offsets
        .get(end.saturating_sub(1))
        .and_then(|offset| offset.checked_add(1))
        .ok_or_else(|| {
            parse_error(
                label,
                instruction.source_line,
                "could not locate FROM image reference",
            )
        })?;
    let kind = parse_base_kind(reference, instruction.source_line, label, stages)?;
    Ok(DockerfileBase {
        kind,
        original_reference: reference.to_owned(),
        instruction_index,
        source_line: instruction.source_line,
        stage,
        reference_range: range_start..range_end,
    })
}

fn parse_base_kind(
    reference: &str,
    source_line: u32,
    label: &str,
    stages: &BTreeSet<String>,
) -> Result<DockerfileBaseKind, MmError> {
    let kind = if reference.eq_ignore_ascii_case("scratch") {
        DockerfileBaseKind::Scratch
    } else if stages.contains(&reference.to_ascii_lowercase()) {
        DockerfileBaseKind::Stage
    } else if reference.contains('$') {
        DockerfileBaseKind::Variable {
            name: parse_variable(reference).ok_or_else(|| {
                parse_error(
                    label,
                    source_line,
                    &format!(
                        "variable FROM reference '{reference}' is unsupported; use exactly $ARG or ${{ARG}} and declare build.depends-on"
                    ),
                )
            })?,
        }
    } else {
        if let Some((_, digest)) = reference.split_once('@')
            && (!crate::domain::build::is_sha256_digest(digest)
                || reference.matches('@').count() != 1)
        {
            return Err(parse_error(
                label,
                source_line,
                &format!("FROM image reference '{reference}' has an invalid OCI digest"),
            ));
        }
        DockerfileBaseKind::Literal {
            repository: super::build::normalize_image_repository(reference).map_err(|_| {
                parse_error(
                    label,
                    source_line,
                    &format!("FROM image reference '{reference}' is not OCI-compatible"),
                )
            })?,
        }
    };
    Ok(kind)
}

fn tokens(value: &str) -> Vec<(&str, usize, usize)> {
    let mut output = Vec::new();
    let mut start = None;
    for (index, character) in value.char_indices() {
        if character.is_whitespace() {
            if let Some(token_start) = start.take() {
                output.push((&value[token_start..index], token_start, index));
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(token_start) = start {
        output.push((&value[token_start..], token_start, value.len()));
    }
    output
}

fn parse_variable(value: &str) -> Option<String> {
    let name = value
        .strip_prefix("${")
        .and_then(|value| value.strip_suffix('}'))
        .or_else(|| value.strip_prefix('$'))?;
    is_variable_name(name).then(|| name.to_owned())
}

fn is_variable_name(value: &str) -> bool {
    value
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_uppercase() || byte == b'_')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

fn is_stage_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

fn parse_error(label: &str, line: u32, message: &str) -> MmError {
    MmError::Build {
        message: format!("{label}:{line}: {message}"),
    }
}

#[cfg(test)]
mod tests;
