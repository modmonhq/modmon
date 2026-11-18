use super::*;
use crate::domain::config::{EnvScalar, ModuleBuildConfig};

pub(super) fn validate_build_targets(
    document: &ConfigDocument,
    module_name: &str,
    build: &ModuleBuildConfig,
    report: &mut ValidationReport,
) {
    let config_file = document.source("modules");
    let mut suffixes = BTreeSet::new();
    for (name, target) in &build.targets {
        let stage = target.stage(name);
        let mut names = vec![(name.as_str(), "build target")];
        if stage != name {
            names.push((stage, "Dockerfile target"));
        }
        for (candidate, what) in names {
            if !is_build_identifier(candidate) {
                report.push(
                    Diagnostic::error(
                        "MM_BUILD_TARGET_INVALID",
                        format!(
                            "module '{module_name}' {what} '{candidate}' is not a safe Docker target"
                        ),
                    )
                    .at_path(config_file),
                );
            }
        }
        let suffix = target.suffix(name);
        if !suffix.is_empty() && !is_build_identifier(suffix) {
            report.push(
                Diagnostic::error(
                    "MM_BUILD_TAG_SUFFIX_INVALID",
                    format!(
                        "module '{module_name}' build tag suffix '{suffix}' is not OCI-tag safe"
                    ),
                )
                .at_path(config_file),
            );
        }
        if !suffixes.insert(suffix) {
            report.push(
                Diagnostic::error(
                    "MM_BUILD_TAG_SUFFIX_DUPLICATE",
                    format!(
                        "module '{module_name}' assigns build tag suffix '{suffix}' more than once; give build target '{name}' its own suffix"
                    ),
                )
                .at_path(config_file),
            );
        }
        validate_build_args(
            document,
            &format!("module '{module_name}' build target '{name}'"),
            target.args(),
            report,
        );
    }
}

pub(super) fn validate_build_args(
    document: &ConfigDocument,
    owner: &str,
    args: &BTreeMap<String, EnvScalar>,
    report: &mut ValidationReport,
) {
    let config_file = document.source("modules");
    for (argument, value) in args {
        let problem = if !is_variable_name(argument) {
            Some((
                "MM_BUILD_ARG_INVALID",
                format!("{owner} build argument '{argument}' is not a valid variable name"),
            ))
        } else if argument == "VERSION" {
            Some((
                "MM_BUILD_ARG_RESERVED",
                format!("{owner} build argument 'VERSION' is supplied automatically from Git"),
            ))
        } else if looks_like_secret_name(argument) {
            Some((
                "MM_BUILD_SECRET_ARG_FORBIDDEN",
                format!("{owner} build argument '{argument}' looks sensitive; use build.secrets"),
            ))
        } else {
            None
        };
        if let Some((code, message)) = problem {
            report.push(Diagnostic::error(code, message).at_path(config_file));
        }
        if !is_safe_environment_value(&value.to_string()) {
            report.push(
                Diagnostic::error(
                    "MM_BUILD_ARG_VALUE_INVALID",
                    format!("{owner} build argument '{argument}' contains control characters"),
                )
                .at_path(config_file),
            );
        }
    }
}

pub(super) fn sets_build_arg(build: &ModuleBuildConfig, arg: &str) -> bool {
    build.args.contains_key(arg)
        || build
            .targets
            .values()
            .any(|target| target.args().contains_key(arg))
}
