//! `config-files` on a service: what `mm config validate` accepts, how a
//! template is rendered, and what names a version.

use std::fs;
use std::path::Path;

use mm::adapters::config::{ConfigLoader, validate};
use mm::adapters::environment::EnvironmentIndexer;
use mm::domain::config_file::{
    BindingProblem, is_version, render, target_problem, template_keys, version,
};
use tempfile::TempDir;

mod temp_dirs;

/// The minimal fixture project with `entries` as the `config-files` of its
/// one service, `api`, whose module directory is `api`.
fn project(entries: &str) -> Result<TempDir, Box<dyn std::error::Error>> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal");
    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path();
    fs::create_dir_all(root.join(".config"))?;
    fs::create_dir_all(root.join("api"))?;
    for file in ["compose.yaml", "api/.env.example"] {
        fs::copy(fixture.join(file), root.join(file))?;
    }
    let config = fs::read_to_string(fixture.join(".config/mm.yaml"))?;
    let service = "      api:\n        stack: app\n";
    assert!(config.contains(service), "the fixture has the api service");
    fs::write(
        root.join(".config/mm.yaml"),
        config.replacen(
            service,
            &format!("{service}        config-files:\n{entries}"),
            1,
        ),
    )?;
    fs::write(
        root.join("api/site.conf.tmpl"),
        "upstream {{ DB.HOST }}:{{DB.PORT}};\n",
    )?;
    fs::write(root.join("api/motd"), "{{ not a template }}\n")?;
    Ok(temporary)
}

/// The `MM_CONFIG_FILE` findings of the project's `local` environment, and
/// whether the project is valid.
fn findings(root: &Path) -> Result<(Vec<String>, bool), Box<dyn std::error::Error>> {
    let document = ConfigLoader.load_from(root)?;
    let report = validate(&document, "local");
    Ok((
        report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "MM_CONFIG_FILE")
            .map(|diagnostic| diagnostic.message.clone())
            .collect(),
        report.is_valid(),
    ))
}

#[test]
fn a_file_and_a_template_of_the_module_validate() -> Result<(), Box<dyn std::error::Error>> {
    let project = project(
        "          /etc/nginx/conf.d/site.conf: {template: site.conf.tmpl}\n          /etc/motd: {file: motd, uid: \"101\", gid: \"101\", mode: 0o440}\n",
    )?;
    let document = ConfigLoader.load_from(project.path())?;
    let files = &document.config.modules["api"].services["api"].config_files;
    assert_eq!(files["/etc/motd"].mode, 0o440, "YAML's 0o440 is octal");
    assert_eq!(
        files["/etc/nginx/conf.d/site.conf"].mode, 0o444,
        "a config file is world-readable unless it says otherwise"
    );
    let (findings, valid) = findings(project.path())?;
    assert!(findings.is_empty(), "{findings:?}");
    assert!(valid);
    Ok(())
}

#[test]
fn a_template_that_names_a_secret_is_refused_with_the_way_to_ship_it()
-> Result<(), Box<dyn std::error::Error>> {
    let project = project("          /etc/app.conf: {template: app.conf.tmpl}\n")?;
    fs::write(
        project.path().join("api/app.conf.tmpl"),
        "host = {{ DB.HOST }}\npassword = {{ DB.PASSWORD }}\n",
    )?;
    let (findings, valid) = findings(project.path())?;
    assert!(!valid);
    assert_eq!(findings.len(), 1, "{findings:?}");
    let finding = &findings[0];
    for expected in [
        "modules.api.services.api.config-files.\"/etc/app.conf\"",
        "'api/app.conf.tmpl' line 2",
        "`{{ DB.PASSWORD }}`",
        "the secret 'database.password'",
        "never takes a secret",
        "secret-files: {database-password: {secret: database.password}}",
        "/run/secrets/database-password",
    ] {
        assert!(finding.contains(expected), "{expected}: {finding}");
    }
    Ok(())
}

#[test]
fn every_refusal_names_the_entry_and_what_to_write() -> Result<(), Box<dyn std::error::Error>> {
    for (entry, expected) in [
        (
            "/etc/a: {template: site.conf.tmpl, file: motd}",
            "needs exactly one source",
        ),
        ("/etc/a: {mode: 292}", "needs exactly one source"),
        (
            "etc/a: {file: motd}",
            "is not an absolute path in the container",
        ),
        ("/etc/a/: {file: motd}", "names a directory"),
        ("/etc/../a: {file: motd}", "`..` part"),
        ("/etc/a: {file: ../compose.yaml}", "inside module 'api'"),
        ("/etc/a: {file: /etc/passwd}", "inside module 'api'"),
        (
            "/etc/a: {file: missing.conf}",
            "'api/missing.conf' does not exist",
        ),
        ("/etc/a: {file: motd, mode: 0}", "is not a file mode"),
        ("/etc/a: {file: motd, mode: 4095}", "is not a file mode"),
        (
            "/etc/a: {file: motd, uid: nginx}",
            "uid 'nginx' is not a numeric id",
        ),
        (
            "/etc/a: {file: motd, gid: \"-1\"}",
            "gid '-1' is not a numeric id",
        ),
        (
            "/etc/a: {template: motd}",
            "'api/motd' line 1 has `{{` that does not start a placeholder",
        ),
        (
            "/etc/a: {template: unknown.tmpl}",
            "environment 'local' has no binding 'APP.NAME'. Add `APP.NAME: {value: ...}` under `bindings:` or `environments.local.bindings:`",
        ),
    ] {
        let project = project(&format!("          {entry}\n"))?;
        fs::write(
            project.path().join("api/unknown.tmpl"),
            "\n\nname {{ APP.NAME }}\n",
        )?;
        let (findings, valid) = findings(project.path())?;
        assert!(!valid, "{entry}");
        assert_eq!(findings.len(), 1, "{entry}: {findings:?}");
        assert!(
            findings[0].starts_with("modules.api.services.api.config-files.\""),
            "{}",
            findings[0]
        );
        assert!(findings[0].contains(expected), "{entry}: {}", findings[0]);
    }
    Ok(())
}

#[test]
fn a_file_that_is_a_symlink_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let project = project("          /etc/a: {file: link}\n")?;
    std::os::unix::fs::symlink("/etc/hostname", project.path().join("api/link"))?;
    let (findings, valid) = findings(project.path())?;
    assert!(!valid);
    assert_eq!(findings.len(), 1, "{findings:?}");
    Ok(())
}

#[test]
fn an_unknown_key_in_an_entry_fails_to_load() -> Result<(), Box<dyn std::error::Error>> {
    let project = project("          /etc/a: {file: motd, owner: root}\n")?;
    let error = ConfigLoader
        .load_from(project.path())
        .err()
        .ok_or("an unknown key loaded")?;
    assert!(error.to_string().contains("owner"), "{error}");
    Ok(())
}

/// The bug commit 7 found: validation passed a Compose file that the host
/// then refused while planning.
#[test]
fn validation_refuses_a_compose_environment_as_the_plan_does()
-> Result<(), Box<dyn std::error::Error>> {
    for key in ["environment: {VERSION: v1}", "env_file: [production.env]"] {
        let project = project("          /etc/motd: {file: motd}\n")?;
        fs::write(
            project.path().join("compose.yaml"),
            format!("services:\n  api:\n    image: docker.io/library/alpine:3.22\n    {key}\n"),
        )?;
        let document = ConfigLoader.load_from(project.path())?;
        let report = validate(&document, "local");
        let refusals = report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "MM_COMPOSE_ENVIRONMENT")
            .collect::<Vec<_>>();
        assert_eq!(refusals.len(), 1, "{:?}", report.diagnostics);
        let message = &refusals[0].message;
        let name = key.split(':').next().unwrap_or_default();
        for expected in [
            "compose.yaml' declares environment or env_file",
            "service 'api'",
            &format!("`{name}`"),
            "`env:` in mm.yaml",
        ] {
            assert!(message.contains(expected), "{expected}: {message}");
        }
        assert_eq!(
            refusals[0].path.as_deref(),
            Some(project.path().join("compose.yaml").as_path()),
            "the finding points at the Compose file"
        );
        assert!(!report.is_valid());
    }
    Ok(())
}

#[test]
fn a_binding_only_a_config_file_template_names_is_not_reported_unused()
-> Result<(), Box<dyn std::error::Error>> {
    let project = project("          /etc/greeting.conf: {template: greeting.conf.tmpl}\n")?;
    let root = project.path();
    fs::write(
        root.join("api/greeting.conf.tmpl"),
        "greeting {{ APP.GREETING }}\n",
    )?;
    let config = fs::read_to_string(root.join(".config/mm.yaml"))?;
    let bindings = "    bindings:\n";
    assert!(config.contains(bindings), "the fixture has bindings");
    fs::write(
        root.join(".config/mm.yaml"),
        config.replacen(
            bindings,
            "    bindings:\n      APP.GREETING:\n        value: hello\n      NOBODY.USES:\n        value: unused\n",
            1,
        ),
    )?;
    let document = ConfigLoader.load_from(root)?;
    let unused: Vec<String> = EnvironmentIndexer
        .index(&document, "local")
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "MM_ENV_BINDING_UNUSED")
        .map(|diagnostic| diagnostic.message.clone())
        .collect();
    assert_eq!(
        unused,
        ["environment binding 'NOBODY.USES' is not referenced by any template"],
        "a binding a config file template names is used"
    );
    Ok(())
}

#[test]
fn template_keys_are_what_a_template_asks_for() {
    assert_eq!(
        template_keys("{{ A.B }} and {{A.B}} then {{ C_1 }}\n"),
        ["A.B", "C_1"]
    );
    assert!(template_keys("no placeholder\n").is_empty());
    assert_eq!(
        template_keys("{{ OK }} {{ not a key }} {{ LATER }}"),
        ["OK"]
    );
}

#[test]
fn a_template_replaces_placeholders_and_nothing_else() {
    let value = |key: &str| match key {
        "APP.URL" => Ok("https://app.example.test".to_owned()),
        "DB.PASSWORD" => Err(BindingProblem::Secret("database.password".to_owned())),
        _ => Err(BindingProblem::Unknown),
    };
    assert_eq!(
        render(
            "server_name {{ APP.URL }};\nset $x ${y}; # {{APP.URL}} { } }}\n",
            value
        )
        .ok(),
        Some(
            "server_name https://app.example.test;\nset $x ${y}; # https://app.example.test { } }}\n"
                .to_owned()
        )
    );
    assert_eq!(
        render("no placeholders", value).ok().as_deref(),
        Some("no placeholders")
    );
    let unknown = render("a\nb {{ APP.NAME }}", value).err();
    assert_eq!(
        unknown.map(|problem| (problem.line, problem.key, problem.problem)),
        Some((
            2,
            Some("APP.NAME".to_owned()),
            Some(BindingProblem::Unknown)
        ))
    );
    let secret = render("{{ DB.PASSWORD }}", value).err();
    assert_eq!(
        secret.and_then(|problem| problem.problem),
        Some(BindingProblem::Secret("database.password".to_owned()))
    );
    for malformed in ["{{ lower.case }}", "{{ APP.URL", "{{}}", "{{ A B }}"] {
        let problem = render(malformed, value).err();
        assert_eq!(
            problem.map(|problem| (problem.line, problem.key)),
            Some((1, None)),
            "{malformed}"
        );
    }
}

#[test]
fn a_version_names_content_owner_and_mode() {
    let base = version(b"listen 80;\n", 0o444, None, None);
    assert!(is_version(&base));
    assert_eq!(base, version(b"listen 80;\n", 0o444, None, None));
    for other in [
        version(b"listen 81;\n", 0o444, None, None),
        version(b"listen 80;\n", 0o440, None, None),
        version(b"listen 80;\n", 0o444, Some(0), None),
        version(b"listen 80;\n", 0o444, None, Some(0)),
        version(b"listen 80;\n", 0o444, Some(101), Some(101)),
    ] {
        assert_ne!(base, other);
        assert!(is_version(&other));
    }
    assert!(!is_version(".tmp"));
    assert!(!is_version(&base.to_uppercase()));
    assert_eq!(target_problem("/etc/nginx/nginx.conf"), None);
    assert!(target_problem("/etc/a:ro").is_some());
    assert!(target_problem("/").is_some());
}
