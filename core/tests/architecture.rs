use std::fs;
use std::path::Path;

mod test_items;

#[test]
fn architecture_domain_does_not_import_adapters() {
    assert_no_layer_imports(
        Path::new("src/domain"),
        "domain",
        &["crate::adapters", "crate::app", "crate::presentation"],
    );
}

#[test]
fn architecture_application_does_not_import_adapters() {
    assert_no_production_layer_imports(
        Path::new("src/app"),
        "application",
        &["crate::adapters", "crate::presentation"],
    );
}

#[test]
fn architecture_presentation_does_not_import_adapters() {
    assert_no_production_adapter_imports(Path::new("src/presentation"), "presentation");
}

#[test]
fn architecture_concrete_composition_has_one_root() {
    let source = rust_files(Path::new("src/composition"))
        .into_iter()
        .map(|path| fs::read_to_string(path).unwrap_or_default())
        .collect::<String>();
    assert!(source.contains("crate::adapters"));
    for root in ["src/app", "src/domain", "src/presentation"] {
        assert_no_production_adapter_imports(Path::new(root), root);
    }
}

#[test]
fn adapters_and_composition_stay_within_handwritten_size_limits() {
    assert_size_limits(Path::new("src/adapters"), "adapter");
    assert_size_limits(Path::new("src/composition"), "composition");
}

#[test]
fn composition_has_no_generic_service_locator() {
    for path in rust_files(Path::new("src/composition")) {
        let source = production_file_source(&path);
        for forbidden in [
            "std::any::Any",
            "dyn Any",
            ".downcast",
            "TypeId",
            "HashMap<String, Box<dyn",
        ] {
            assert!(
                !source.contains(forbidden),
                "composition file {} contains service-locator primitive '{forbidden}'",
                path.display()
            );
        }
    }
}

#[test]
fn inner_layers_do_not_name_runtime_executables_or_driver_ids() {
    for root in [
        Path::new("src/app"),
        Path::new("src/domain"),
        Path::new("src/presentation"),
    ] {
        for path in rust_files(root) {
            let source = production_file_source(&path);
            for forbidden in [
                "CommandSpec::new(\"docker\")",
                "CommandSpec::new(\"podman\")",
                "CommandSpec::new(\"vault\")",
                "CommandSpec::new(\"bao\")",
                "ReqwestVaultTransport",
                "ReqwestOpenBaoTransport",
            ] {
                assert!(
                    !source.contains(forbidden),
                    "inner-layer file {} contains concrete boundary name '{forbidden}'",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn provider_presentation_has_no_concrete_driver_dispatch_or_name_inference() {
    for path in rust_files(Path::new("src/presentation")) {
        let source = production_file_source(&path);
        for forbidden in [
            "BackendFlavor",
            "OpenBaoCommand",
            "VaultCommand",
            "openbao-external",
            "openbao-managed",
            "vault-external",
            "vault-managed",
            "starts_with(\"vault",
            "starts_with(\"openbao",
        ] {
            assert!(
                !source.contains(forbidden),
                "presentation file {} contains forbidden provider inference '{forbidden}'",
                path.display()
            );
        }
    }
}

#[test]
fn managed_provider_drivers_have_no_engine_process_construction() {
    for root in [
        Path::new("src/adapters/secrets/openbao"),
        Path::new("src/adapters/secrets/vault"),
    ] {
        for path in rust_files(root) {
            let source = production_file_source(&path);
            for forbidden in [
                "CommandSpec::new(\"docker\")",
                "CommandSpec::new(\"podman\")",
                "docker_owned",
                "podman_owned",
            ] {
                assert!(
                    !source.contains(forbidden),
                    "managed provider file {} constructs an engine process through '{forbidden}'",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn presentation_stays_within_handwritten_size_limits() {
    assert_size_limits(Path::new("src/presentation"), "presentation");
}

#[test]
fn application_and_domain_stay_within_handwritten_size_limits() {
    assert_size_limits(Path::new("src/app"), "application");
    assert_size_limits(Path::new("src/domain"), "domain");
}

#[test]
fn application_and_domain_do_not_use_presentation_contracts() {
    for root in [Path::new("src/app"), Path::new("src/domain")] {
        for path in rust_files(root) {
            let source = production_file_source(&path);
            for forbidden in ["clap::", "axum::", "ratatui::", "crossterm::", "rmcp::"] {
                assert!(
                    !source.contains(forbidden),
                    "core file {} contains presentation contract '{forbidden}'",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn application_and_domain_have_no_size_or_dead_code_suppressions() {
    for root in [Path::new("src/app"), Path::new("src/domain")] {
        for path in rust_files(root) {
            let source = fs::read_to_string(&path).unwrap_or_default();
            for forbidden in ["clippy::too_many_arguments", "allow(dead_code)"] {
                assert!(
                    !source.contains(forbidden),
                    "core file {} contains forbidden suppression '{forbidden}'",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn generic_application_services_do_not_infer_provider_names() {
    for path in rust_files(Path::new("src/app")) {
        let source = production_file_source(&path);
        for forbidden in [
            "== \"vault\"",
            "== \"openbao\"",
            "starts_with(\"vault",
            "starts_with(\"openbao",
        ] {
            assert!(
                !source.contains(forbidden),
                "application file {} contains provider-name inference '{forbidden}'",
                path.display()
            );
        }
    }
}

#[test]
fn pure_domain_has_no_process_or_async_runtime_dependencies() {
    for path in rust_files(Path::new("src/domain")) {
        let source = production_file_source(&path);
        for forbidden in ["tokio::", "async_trait", "process_wrap::", "std::process"] {
            assert!(
                !source.contains(forbidden),
                "domain file {} contains effect dependency '{forbidden}'",
                path.display()
            );
        }
        if path
            .file_name()
            .is_some_and(|name| name != "path_policy.rs")
        {
            assert!(
                !source.contains("std::fs"),
                "pure domain file {} imports filesystem effects",
                path.display()
            );
        }
    }
}

fn assert_size_limits(root: &Path, layer: &str) {
    let mut violations = Vec::new();
    for path in rust_files(root) {
        if is_test_source(&path) {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap_or_default();
        let lines = source.lines().count();
        if lines > 800 {
            violations.push(format!(
                "handwritten {layer} file {} has {lines} lines; hard limit is 800",
                path.display()
            ));
        }
        for (start, end) in function_ranges(&production_file_source(&path)) {
            let length = end - start + 1;
            if length > 100 {
                violations.push(format!(
                    "{layer} function at {}:{start} has {length} lines; hard limit is 100",
                    path.display()
                ));
            }
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

#[test]
fn presentation_has_no_size_or_dead_code_suppressions() {
    for path in rust_files(Path::new("src/presentation")) {
        let source = fs::read_to_string(&path).unwrap_or_default();
        for forbidden in ["clippy::too_many_arguments", "allow(dead_code)"] {
            assert!(
                !source.contains(forbidden),
                "presentation file {} contains forbidden suppression '{forbidden}'",
                path.display()
            );
        }
    }
}

#[test]
fn architecture_unstable_modules_are_feature_gated() {
    let manifest = fs::read_to_string("Cargo.toml").unwrap_or_default();
    let library = fs::read_to_string("src/lib.rs").unwrap_or_default();
    assert!(manifest.contains("unstable = []"));
    assert!(manifest.contains("required-features = [\"unstable\"]"));
    // The domain layer is a crate of its own, re-exported under the old name.
    for (module, public) in [
        ("adapters", "pub mod adapters;"),
        ("app", "pub mod app;"),
        ("domain", "pub use mm_domain::domain;"),
        ("presentation", "pub mod presentation;"),
    ] {
        let offset = library.find(public).unwrap_or(0);
        let prefix = &library[..offset];
        assert!(
            prefix.ends_with("#[cfg(feature = \"unstable\")]\n"),
            "public module {module} is not gated by the unstable feature"
        );
        assert_eq!(
            library.matches(public).count(),
            1,
            "module {module} is made public more than once"
        );
    }
    assert!(
        !library.contains("pub use mm_domain::*") && !library.contains("pub extern crate"),
        "the domain crate is re-exported without the unstable gate"
    );
}

#[test]
fn architecture_domain_crate_is_the_domain_directory_and_nothing_else() {
    let root = fs::read_to_string("domain/lib.rs").unwrap_or_default();
    assert!(root.contains("#[path = \"../src/domain/mod.rs\"]\npub mod domain;"));
    let manifest = fs::read_to_string("domain/Cargo.toml").unwrap_or_default();
    assert!(manifest.contains("publish = false"));
    // A crate cannot depend on the crate that depends on it, so the compiler
    // keeps the domain free of the other layers. This keeps it free of the
    // async runtime and the process and network crates as well.
    for forbidden in [
        "mm =",
        "tokio",
        "reqwest",
        "axum",
        "rusqlite",
        "process-wrap",
    ] {
        assert!(
            !manifest.lines().any(|line| line.starts_with(forbidden)),
            "the domain crate depends on {forbidden}"
        );
    }
}

fn assert_no_layer_imports(root: &Path, layer: &str, forbidden: &[&str]) {
    for path in rust_files(root) {
        let source = fs::read_to_string(&path).unwrap_or_default();
        for import in forbidden {
            assert!(
                !source.contains(import),
                "{layer} file {} imports {import}",
                path.display()
            );
        }
    }
}

fn assert_no_production_adapter_imports(root: &Path, layer: &str) {
    assert_no_production_layer_imports(root, layer, &["crate::adapters"]);
}

fn assert_no_production_layer_imports(root: &Path, layer: &str, forbidden: &[&str]) {
    for path in rust_files(root) {
        let source = production_file_source(&path);
        for import in forbidden {
            assert!(
                !source.contains(import),
                "{layer} file {} imports {import}",
                path.display()
            );
        }
    }
}

fn production_file_source(path: &Path) -> String {
    if is_test_source(path) {
        return String::new();
    }
    test_items::without_test_items(&fs::read_to_string(path).unwrap_or_default())
}

fn is_test_source(path: &Path) -> bool {
    path.file_stem().is_some_and(|name| name == "tests")
        || path
            .components()
            .any(|component| component.as_os_str() == "tests")
}

fn rust_files(root: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(rust_files(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    files
}

fn function_ranges(source: &str) -> Vec<(usize, usize)> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut ranges = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if !function_start(lines[index]) {
            index += 1;
            continue;
        }
        let start = index + 1;
        let mut depth = 0_i32;
        let mut opened = false;
        while index < lines.len() {
            if !opened && lines[index].trim_end().ends_with(';') {
                break;
            }
            let (opens, closes) = structural_braces(lines[index]);
            if opens > 0 {
                opened = true;
            }
            depth += opens - closes;
            if opened && depth == 0 {
                ranges.push((start, index + 1));
                break;
            }
            index += 1;
        }
        index += 1;
    }
    ranges
}

fn function_start(line: &str) -> bool {
    let line = line.split("//").next().unwrap_or_default().trim_start();
    line.starts_with("fn ")
        || line.starts_with("async fn ")
        || line.starts_with("const fn ")
        || line.starts_with("pub fn ")
        || line.starts_with("pub async fn ")
        || line.starts_with("pub const fn ")
        || line.starts_with("pub(crate) fn ")
        || line.starts_with("pub(crate) async fn ")
        || line.starts_with("pub(super) fn ")
        || line.starts_with("pub(super) async fn ")
        || line.starts_with("pub(in ") && line.contains(" fn ")
}

fn structural_braces(line: &str) -> (i32, i32) {
    let mut opens = 0;
    let mut closes = 0;
    let mut quoted = false;
    let mut escaped = false;
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        if quoted {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quoted = false;
            }
            continue;
        }
        if character == '/' && characters.peek() == Some(&'/') {
            break;
        }
        if character == '"' {
            quoted = true;
        } else if character == '{' {
            opens += 1;
        } else if character == '}' {
            closes += 1;
        }
    }
    (opens, closes)
}
