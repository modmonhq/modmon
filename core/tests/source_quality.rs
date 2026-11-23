use std::fs;
use std::path::{Path, PathBuf};

mod test_items;

#[test]
fn production_source_has_no_silent_failure_or_panic_shortcuts() -> Result<(), std::io::Error> {
    let mut violations = Vec::new();
    for path in rust_files(Path::new("src"))? {
        if is_test_source(&path) {
            continue;
        }
        let source = fs::read_to_string(&path)?;
        let code = production_code(&source);
        for (index, line) in code.lines().enumerate() {
            for forbidden in [
                ".unwrap(",
                ".expect(",
                "panic!(",
                "todo!(",
                "unimplemented!(",
                "#[allow(",
            ] {
                if line.contains(forbidden) {
                    violations.push(format!(
                        "{}:{} contains forbidden production source '{forbidden}'",
                        path.display(),
                        index + 1
                    ));
                }
            }
            let tokens = line.split_whitespace().collect::<Vec<_>>();
            if tokens.windows(3).any(|tokens| tokens == ["let", "_", "="]) {
                violations.push(format!(
                    "{}:{} anonymously discards a value; use a named cleanup/send policy",
                    path.display(),
                    index + 1
                ));
            }
            if result_sensitive_path(&path) && line.contains(".ok()") {
                violations.push(format!(
                    "{}:{} converts a sensitive Result to absence without an explicit match",
                    path.display(),
                    index + 1
                ));
            }
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
    Ok(())
}

#[test]
fn inner_layers_do_not_import_concrete_boundary_implementations() -> Result<(), std::io::Error> {
    let mut violations = Vec::new();
    for root in ["src/app", "src/domain", "src/presentation"] {
        for path in rust_files(Path::new(root))? {
            if is_test_source(&path) {
                continue;
            }
            let source = fs::read_to_string(&path)?;
            let code = production_code(&source);
            for forbidden in [
                "crate::adapters",
                "CommandSpec::new(docker",
                "CommandSpec::new(podman",
            ] {
                if code.contains(forbidden) {
                    violations.push(format!(
                        "{} imports or constructs concrete boundary '{forbidden}'",
                        path.display()
                    ));
                }
            }
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
    Ok(())
}

/// A blocking file lock taken on a thread of an async runtime stops every
/// task queued behind that thread, for ever when the lock's holder is one
/// of them: that wedged `mmd`. The one place that waits for a file
/// lock is `adapters/file_lock.rs`, and every wait there has a deadline.
#[test]
fn file_locks_are_waited_for_in_one_place_with_a_deadline() -> Result<(), std::io::Error> {
    const BLOCKING: [&str; 9] = [
        "FileExt::lock(",
        "FileExt::lock_shared(",
        "FileExt::lock_exclusive(",
        "File::lock(",
        "File::lock_shared(",
        ".lock_shared(",
        ".lock_exclusive(",
        "flock(",
        "SETLKW",
    ];
    let mut violations = Vec::new();
    for path in rust_files(Path::new("src"))? {
        if is_test_source(&path) || path.ends_with("adapters/file_lock.rs") {
            continue;
        }
        let source = fs::read_to_string(&path)?;
        let code = production_code(&source);
        for (index, line) in code.lines().enumerate() {
            for forbidden in BLOCKING {
                if line.contains(forbidden) {
                    violations.push(format!(
                        "{}:{} waits for a file lock with '{forbidden}'; use crate::adapters::file_lock, which never waits without a deadline",
                        path.display(),
                        index + 1
                    ));
                }
            }
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
    // A method call on a file (`file.lock()`) reads like a mutex's; clippy
    // tells them apart by type, with this configuration.
    let clippy = fs::read_to_string("../clippy.toml")?;
    for method in [
        "std::fs::File::lock\"",
        "std::fs::File::lock_shared\"",
        "fs4::FileExt::lock\"",
        "fs4::FileExt::lock_shared\"",
    ] {
        assert!(
            clippy.contains(method),
            "clippy.toml no longer disallows {method}"
        );
    }
    // The helper itself only ever tries.
    let helper = fs::read_to_string("src/adapters/file_lock.rs")?;
    let helper = code_without_comments_or_literals(&helper);
    assert!(helper.contains("FileExt::try_lock("));
    assert!(BLOCKING.iter().all(|blocking| !helper.contains(blocking)));
    Ok(())
}

/// The one place that starts child processes: the supervised runner,
/// which drains a child's output, enforces its deadline from a thread of its
/// own, and ends its process group. Everything else starts its programs
/// through `ProcessRunner` / `StreamingProcessRunner`. The other files below
/// start programs on a workstation for one command and never inside a
/// request `mmd` serves; a new entry needs the same reason.
#[test]
fn child_processes_are_started_in_one_place() -> Result<(), std::io::Error> {
    const ONE_PLACE: &str = "adapters/process/";
    const ONE_SHOT: [(&str, &str); 17] = [
        (
            "adapters/remote/client.rs",
            "the client's own `ssh` to a host",
        ),
        (
            "adapters/remote/client/reuse.rs",
            "the client's kept `ssh` connections",
        ),
        (
            "adapters/remote/push.rs",
            "the client's `git` that builds a commit's bundle for `delivery: push`",
        ),
        (
            "adapters/keys/askpass.rs",
            "`mkfifo` for the client's passphrase helper",
        ),
        (
            "adapters/keys/managed.rs",
            "the client's private `ssh-agent`",
        ),
        ("adapters/keys/openssh.rs", "`ssh-keygen` for `mm key`"),
        (
            "adapters/keys/vault.rs",
            "the desktop keychain for `mm key`",
        ),
        (
            "adapters/agent_clients.rs",
            "`mm agent` setup on a workstation",
        ),
        ("adapters/macos_host.rs", "local development on macOS"),
        (
            "adapters/remote/access/deploy_key.rs",
            "`ssh-keygen -y` for `mm target setup deploy-key`",
        ),
        (
            "adapters/target/privileged_ingress/install.rs",
            "installing the local ingress helper",
        ),
        (
            "adapters/target/privileged_ingress/platform.rs",
            "the local ingress helper",
        ),
        (
            "adapters/target/privileged_ingress/selinux.rs",
            "the local ingress helper",
        ),
        (
            "adapters/target/privileged_ingress/validation.rs",
            "the local ingress helper",
        ),
        (
            "composition/cluster_setup.rs",
            "the client's `git` for `mm target setup`",
        ),
        (
            "composition/daemon_checkout.rs",
            "the client's `git` for the commit it deploys",
        ),
        (
            "presentation/cli/internal.rs",
            "no child process: `clap::Command`",
        ),
    ];
    let mut violations = Vec::new();
    for path in rust_files(Path::new("src"))? {
        let shown = path.to_string_lossy().replace('\\', "/");
        if is_test_source(&path)
            || shown.ends_with("_tests.rs")
            || shown.contains(ONE_PLACE)
            || ONE_SHOT.iter().any(|(file, _)| shown.ends_with(file))
        {
            continue;
        }
        let source = fs::read_to_string(&path)?;
        let code = production_code(&source);
        for (index, line) in code.lines().enumerate() {
            let starts = line.match_indices("Command::new(").any(|(at, _)| {
                let before = &line[..at];
                !before.ends_with("Spec") && !before.ends_with("clap::")
            });
            let names = line.contains("process::Command")
                || line.contains("CommandWrap")
                || (line.contains("process::{") && line.contains("Command"))
                || line.contains("fork(")
                || line.contains("posix_spawn");
            if starts || names {
                violations.push(format!(
                    "{}:{} starts a child process outside {ONE_PLACE}; build a CommandSpec and run it through the ProcessRunner, which enforces its deadline and ends its process group",
                    path.display(),
                    index + 1
                ));
            }
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
    // Every listed file still exists: a stale entry hides nothing, but it
    // misleads the next reader.
    for (file, why) in ONE_SHOT {
        assert!(
            Path::new("src").join(file).is_file(),
            "{file} ({why}) is gone; take it off the list"
        );
    }
    Ok(())
}

#[test]
fn public_errors_cross_async_boundaries_safely() {
    fn assert_error<T: std::error::Error + Send + Sync + 'static>() {}
    assert_error::<mm::MmError>();
}

#[test]
fn source_scanner_preserves_code_after_lifetimes() {
    let source = "fn borrowed<'a>(value: &'a str) { let _ = value; }";
    assert!(code_without_comments_or_literals(source).contains("let _ = value"));
    assert!(
        !code_without_comments_or_literals("let text = \"let _ = hidden\";")
            .contains("let _ = hidden")
    );
}

#[test]
fn source_scanner_skips_only_the_test_items() {
    let source = "#[cfg(test)]\nfn helper() -> Result<[u8; 2], ()> { Ok([0; 2]) }\n\nfn shipped() { let _ = 1; }\n\n#[cfg(test)]\nmod tests;\n";
    let code = production_code(source);
    assert!(!code.contains("helper") && !code.contains("Ok("));
    assert!(!code.contains("mod tests"));
    assert_eq!(
        code.lines().position(|line| line.contains("let _ = 1")),
        Some(3)
    );
    let fields = "struct Kept {\n    #[cfg(test)]\n    status: Option<u8>\n}\nenum Platform {\n    #[cfg(test)]\n    Test,\n    Linux,\n}\nfn pick(platform: Platform) -> u8 {\n    match platform {\n        #[cfg(test)]\n        Platform::Test => 0,\n        Platform::Linux => 1,\n    };\n    let _ = 2;\n}\n";
    let code = production_code(fields);
    assert!(!code.contains("status") && !code.contains("Test"));
    assert!(code.contains("Linux,") && code.contains("Platform::Linux => 1"));
    assert!(code.contains("let _ = 2"));
}

fn production_code(source: &str) -> String {
    code_without_comments_or_literals(&test_items::without_test_items(source))
}

fn result_sensitive_path(path: &Path) -> bool {
    let path = path.to_string_lossy();
    [
        "src/app/",
        "src/adapters/ca/",
        "src/adapters/runtime/",
        "src/adapters/secrets/",
        "src/adapters/state/",
        "src/presentation/http/",
    ]
    .iter()
    .any(|root| path.starts_with(root))
}

fn is_test_source(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "tests.rs")
        || path
            .components()
            .any(|component| component.as_os_str() == "tests")
}

fn rust_files(root: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut files = Vec::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            files.extend(rust_files(&path)?);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    Ok(files)
}

fn code_without_comments_or_literals(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut characters = source.chars().peekable();
    let mut block_depth = 0_u32;
    let mut quoted = None;
    let mut escaped = false;
    while let Some(character) = characters.next() {
        if block_depth > 0 {
            if character == '/' && characters.peek() == Some(&'*') {
                characters.next();
                block_depth += 1;
            } else if character == '*' && characters.peek() == Some(&'/') {
                characters.next();
                block_depth -= 1;
            }
            output.push(if character == '\n' { '\n' } else { ' ' });
            continue;
        }
        if let Some(quote) = quoted {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == quote {
                quoted = None;
            }
            output.push(if character == '\n' { '\n' } else { ' ' });
            continue;
        }
        if character == '/' && characters.peek() == Some(&'/') {
            characters.next();
            for comment in characters.by_ref() {
                if comment == '\n' {
                    output.push('\n');
                    break;
                }
            }
        } else if character == '/' && characters.peek() == Some(&'*') {
            characters.next();
            block_depth = 1;
            output.push(' ');
        } else if character == '"' || (character == '\'' && starts_character_literal(&characters)) {
            quoted = Some(character);
            output.push(' ');
        } else {
            output.push(character);
        }
    }
    output
}

fn starts_character_literal(characters: &std::iter::Peekable<std::str::Chars<'_>>) -> bool {
    let mut lookahead = characters.clone();
    match lookahead.next() {
        Some('\\') => match lookahead.next() {
            Some('x') => {
                lookahead.next().is_some()
                    && lookahead.next().is_some()
                    && lookahead.next() == Some('\'')
            }
            Some('u') if lookahead.next() == Some('{') => {
                lookahead.any(|character| character == '}') && lookahead.next() == Some('\'')
            }
            Some(_) => lookahead.next() == Some('\''),
            None => false,
        },
        Some('\n' | '\r' | '\'') | None => false,
        Some(_) => lookahead.next() == Some('\''),
    }
}
