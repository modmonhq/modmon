//! The `file` secret provider: a secret read, when it is needed, from a
//! file on the machine that runs mm, verbatim, never from mm's own home and
//! never by a host.

mod temp_dirs;

use std::fs;
use std::path::Path;

use mm::adapters::secrets::{FileSecretStore, HostFileSecrets, file_storage_descriptor};
use mm::app::ports::{SecretMaterialResolver, SecretMetadataInspector};
use mm::domain::secret::{
    FileSecretLocation, SecretReference, SecretScope, SecretStatus, file_secret_location,
};
use mm::domain::secret_contract::{AuthenticationKind, StorageOperation};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const PASSWORD: &str = "file-provider-test-value";
const AUTH: &str = "{\n    \"http-basic\": {\n        \"registry.example.test\": {\n            \"username\": \"ci\",\n            \"password\": \"file-provider-test-value\"\n        }\n    }\n}\n";

fn reference(path: &Path) -> TestResult<SecretReference> {
    Ok(format!("file://{}", path.display()).parse()?)
}

fn scope() -> SecretScope {
    SecretScope::new("local")
}

fn store(directory: &Path) -> FileSecretStore {
    FileSecretStore::new([7; 32], directory.join("mm-home"))
}

async fn state(store: &FileSecretStore, path: &Path) -> TestResult<(SecretStatus, Option<String>)> {
    let reference = reference(path)?;
    let resolution = store.resolve(&scope(), &reference).await?;
    if resolution.metadata.status != SecretStatus::Set {
        assert!(resolution.material.is_none());
    }
    Ok((
        resolution.metadata.status,
        store.invalid_reason(&scope(), &reference).await,
    ))
}

#[tokio::test]
async fn a_file_secret_is_the_file_verbatim_and_its_revision_follows_the_content() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let path = directory.path().join("auth.json");
    fs::write(&path, AUTH)?;
    let store = store(directory.path());
    let reference = reference(&path)?;

    let resolution = store.resolve(&scope(), &reference).await?;
    assert_eq!(resolution.metadata.status, SecretStatus::Set);
    let material = resolution
        .material
        .ok_or("a set file secret has no value")?;
    assert!(material.expose(|value| value == AUTH));
    let revision = resolution.metadata.revision.ok_or("no revision")?;
    assert!(revision.starts_with("keyed-blake3:"));
    assert!(!revision.contains(PASSWORD));
    let inspected = store.inspect(&scope(), &reference).await?;
    assert_eq!(inspected.revision.as_deref(), Some(revision.as_str()));
    assert!(!format!("{store:?}").contains("7, 7"));

    fs::write(&path, AUTH.replace("\"ci\"", "\"deploy\""))?;
    let changed = store.inspect(&scope(), &reference).await?;
    assert_eq!(changed.status, SecretStatus::Set);
    assert_ne!(changed.revision.as_deref(), Some(revision.as_str()));

    fs::write(&path, "x".repeat(1024 * 1024))?;
    assert_eq!(state(&store, &path).await?.0, SecretStatus::Set);
    Ok(())
}

#[tokio::test]
async fn a_missing_or_empty_file_is_unset() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let store = store(directory.path());
    let missing = directory.path().join("missing.json");
    assert_eq!(state(&store, &missing).await?, (SecretStatus::Unset, None));
    let empty = directory.path().join("empty.json");
    fs::write(&empty, "")?;
    assert_eq!(state(&store, &empty).await?, (SecretStatus::Unset, None));
    let under_a_file = empty.join("auth.json");
    assert_eq!(
        state(&store, &under_a_file).await?,
        (SecretStatus::Unset, None)
    );
    Ok(())
}

#[tokio::test]
async fn a_directory_a_large_file_and_binary_content_are_invalid_and_say_why() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let store = store(directory.path());
    let cases = [
        ("directory", "is a directory, not a file"),
        ("large.json", "is larger than 1 MiB"),
        ("binary.bin", "is not UTF-8 text"),
    ];
    fs::create_dir(directory.path().join("directory"))?;
    fs::write(
        directory.path().join("large.json"),
        "x".repeat(1024 * 1024 + 1),
    )?;
    fs::write(
        directory.path().join("binary.bin"),
        [0xff, 0xfe, 0x00, 0x41],
    )?;
    for (name, reason) in cases {
        let path = directory.path().join(name);
        let (status, said) = state(&store, &path).await?;
        assert_eq!(status, SecretStatus::Invalid, "{name}");
        let said = said.ok_or("an invalid file secret gave no reason")?;
        assert!(said.contains(reason), "{name}: {said}");
        assert!(said.contains(&path.display().to_string()), "{said}");
    }
    Ok(())
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_named_pipe_is_invalid_and_never_waited_on() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let path = directory.path().join("pipe");
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &path,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        0,
    )?;
    let store = store(directory.path());
    let (status, reason) = state(&store, &path).await?;
    assert_eq!(status, SecretStatus::Invalid);
    assert!(reason.is_some_and(|reason| reason.contains("is not a regular file")));
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn files_in_mm_homes_are_refused_even_through_a_link_and_other_links_are_followed()
-> TestResult {
    let directory = temp_dirs::tempdir()?;
    let mm_home = directory.path().join("mm-home");
    fs::create_dir_all(mm_home.join("identities"))?;
    let key = mm_home.join("identities/age.key");
    fs::write(&key, "AGE-SECRET-KEY-TEST")?;
    let store = store(directory.path());

    let (status, reason) = state(&store, &key).await?;
    assert_eq!(status, SecretStatus::Invalid);
    let reason = reason.ok_or("no reason")?;
    assert!(reason.contains("inside mm's home"), "{reason}");
    assert!(!reason.contains("AGE-SECRET-KEY-TEST"));

    let link = directory.path().join("innocent.json");
    std::os::unix::fs::symlink(&key, &link)?;
    assert_eq!(state(&store, &link).await?.0, SecretStatus::Invalid);
    let home_link = directory.path().join("home-link");
    std::os::unix::fs::symlink(&mm_home, &home_link)?;
    assert_eq!(
        state(&store, &home_link.join("identities/age.key"))
            .await?
            .0,
        SecretStatus::Invalid
    );

    let auth = directory.path().join("auth.json");
    fs::write(&auth, AUTH)?;
    let followed = directory.path().join("followed.json");
    std::os::unix::fs::symlink(&auth, &followed)?;
    let resolution = store.resolve(&scope(), &reference(&followed)?).await?;
    assert_eq!(resolution.metadata.status, SecretStatus::Set);
    assert!(
        resolution
            .material
            .is_some_and(|material| material.expose(|value| value == AUTH))
    );
    Ok(())
}

#[tokio::test]
async fn a_relative_location_is_refused_with_what_to_write_instead() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let store = store(directory.path());
    let relative: SecretReference = "file://secrets/auth.json".parse()?;
    let error = store
        .resolve(&scope(), &relative)
        .await
        .err()
        .ok_or("a relative file location was read")?
        .to_string();
    assert!(error.contains("committed"), "{error}");
    assert!(error.contains("~/.config/tool/auth.json"), "{error}");

    assert_eq!(
        file_secret_location("/etc/tool/auth.json"),
        Ok(FileSecretLocation::Absolute(Path::new(
            "/etc/tool/auth.json"
        )))
    );
    assert_eq!(
        file_secret_location("~/.config/tool/auth.json"),
        Ok(FileSecretLocation::Home(Path::new(
            ".config/tool/auth.json"
        )))
    );
    for refused in [
        "auth.json",
        "./auth.json",
        "~",
        "~/",
        "~//etc/auth.json",
        "~user/auth.json",
    ] {
        assert!(file_secret_location(refused).is_err(), "{refused}");
    }
    Ok(())
}

#[tokio::test]
async fn a_host_never_reads_a_file_secret_and_says_how_to_stage_the_value() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let path = directory.path().join("auth.json");
    fs::write(&path, AUTH)?;
    let reference = reference(&path)?;
    let scope = SecretScope::new("uat");
    let host = HostFileSecrets;
    let refused = host
        .resolve(&scope, &reference)
        .await
        .err()
        .ok_or("a host read a file secret")?
        .to_string();
    assert!(refused.contains("a host never reads a file"), "{refused}");
    assert!(refused.contains("provider: age"), "{refused}");
    assert!(
        refused.contains(&format!(
            "mm -e uat secret set NAME --stdin < {}",
            path.display()
        )),
        "{refused}"
    );
    assert!(!refused.contains(PASSWORD));
    assert!(host.inspect(&scope, &reference).await.is_err());
    Ok(())
}

#[test]
fn the_file_provider_is_read_only_text_up_to_one_mebibyte() -> TestResult {
    let descriptor = file_storage_descriptor()?;
    assert_eq!(descriptor.id.as_str(), "file");
    assert_eq!(
        descriptor.operations,
        [
            StorageOperation::InspectMetadata,
            StorageOperation::ResolveMaterial
        ]
    );
    assert_eq!(descriptor.max_material_bytes, 1024 * 1024);
    assert_eq!(descriptor.authentication, AuthenticationKind::LocalFile);
    Ok(())
}

const FILE_SECRET: &str = "  registry.auth:\n    storage:\n      provider: file\n      location: ~/.config/tool/auth.json\n";

fn project(secrets: &str) -> TestResult<tempfile::TempDir> {
    let project = temp_dirs::tempdir()?;
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal");
    fs::create_dir_all(project.path().join(".config"))?;
    fs::create_dir_all(project.path().join("api"))?;
    for file in [".config/mm.yaml", "compose.yaml", "api/.env.example"] {
        fs::copy(fixture.join(file), project.path().join(file))?;
    }
    let config = project.path().join(".config/mm.yaml");
    let text = fs::read_to_string(&config)?
        .replace("environments:\n", &format!("{secrets}environments:\n"));
    fs::write(config, text)?;
    Ok(project)
}

fn mm(home: &Path, project: &Path, arguments: &[&str]) -> assert_cmd::Command {
    let mut command = temp_dirs::mm();
    command
        .env("HOME", home)
        .env_remove("MM_HOME")
        .arg("--root")
        .arg(project)
        .args(arguments);
    command
}

fn json_status(output: &std::process::Output) -> TestResult<serde_json::Value> {
    let stdout = String::from_utf8(output.stdout.clone())?;
    assert!(!stdout.contains(PASSWORD), "{stdout}");
    Ok(serde_json::from_str(&stdout)?)
}

#[test]
fn status_names_the_file_and_its_state_and_never_the_value() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let project = project(FILE_SECRET)?;
    let auth = home.path().join(".config/tool/auth.json");
    fs::create_dir_all(home.path().join(".config/tool"))?;
    fs::write(&auth, AUTH)?;
    let status = ["--output", "json", "secret", "status", "registry.auth"];

    let set = json_status(&mm(home.path(), project.path(), &status).output()?)?;
    assert_eq!(set["secrets"][0]["key"], "registry.auth");
    assert_eq!(set["secrets"][0]["provider"], "file");
    assert_eq!(set["secrets"][0]["location"], "~/.config/tool/auth.json");
    assert_eq!(set["secrets"][0]["status"], "set");

    fs::remove_file(&auth)?;
    let unset = json_status(&mm(home.path(), project.path(), &status).output()?)?;
    assert_eq!(unset["secrets"][0]["status"], "unset");
    let text = mm(
        home.path(),
        project.path(),
        &["secret", "status", "registry.auth"],
    )
    .output()?;
    let text = String::from_utf8(text.stdout)?;
    assert!(text.contains("unset"), "{text}");
    assert!(text.contains("~/.config/tool/auth.json"), "{text}");
    Ok(())
}

#[test]
fn set_and_rotate_refuse_a_file_secret_and_name_the_file() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let project = project(FILE_SECRET)?;
    for command in [
        ["secret", "set", "registry.auth", "--stdin"],
        ["secret", "rotate", "registry.auth", "--stdin"],
    ] {
        let output = mm(home.path(), project.path(), &command)
            .write_stdin("replacement-value")
            .output()?;
        assert!(!output.status.success());
        let stderr = String::from_utf8(output.stderr)?;
        assert!(
            stderr.contains(
                "secret 'registry.auth' is read from ~/.config/tool/auth.json; edit that file"
            ),
            "{stderr}"
        );
    }
    Ok(())
}

#[test]
fn a_relative_file_location_fails_validation_and_says_what_to_write() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let project = project(&FILE_SECRET.replace("~/.config/tool/auth.json", "secrets/auth.json"))?;
    let output = mm(home.path(), project.path(), &["config", "validate"]).output()?;
    assert!(!output.status.success());
    let said = format!(
        "{}{}",
        String::from_utf8(output.stdout)?,
        String::from_utf8(output.stderr)?
    );
    assert!(said.contains("MM_SECRET_REF_INVALID"), "{said}");
    assert!(said.contains("registry.auth"), "{said}");
    assert!(said.contains("~/.config/tool/auth.json"), "{said}");
    Ok(())
}

#[test]
fn a_file_in_mm_s_own_home_is_invalid_and_never_read() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let in_home = project(&FILE_SECRET.replace(
        "~/.config/tool/auth.json",
        "~/.mm/identities/environment-revisions.key",
    ))?;
    let output = mm(
        home.path(),
        in_home.path(),
        &["--output", "json", "secret", "status", "registry.auth"],
    )
    .output()?;
    let status = json_status(&output)?;
    assert_eq!(status["secrets"][0]["status"], "invalid");
    let reasons = status["diagnostics"].to_string();
    assert!(reasons.contains("inside mm's home"), "{reasons}");

    let elsewhere = temp_dirs::tempdir()?;
    let key = elsewhere
        .path()
        .join("identities/environment-revisions.key");
    let absolute =
        project(&FILE_SECRET.replace("~/.config/tool/auth.json", &key.display().to_string()))?;
    let mm_home = elsewhere.path().display().to_string();
    let output = mm(
        home.path(),
        absolute.path(),
        &[
            "--mm-home",
            &mm_home,
            "--output",
            "json",
            "secret",
            "status",
            "registry.auth",
        ],
    )
    .output()?;
    assert_eq!(json_status(&output)?["secrets"][0]["status"], "invalid");
    Ok(())
}

#[test]
fn an_env_secret_with_a_newline_is_invalid_and_points_to_the_file_provider() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let project = project(
        "  ci.token:\n    storage:\n      provider: env\n      location: MM_FILE_PROVIDER_TEST_TOKEN\n",
    )?;
    let output = mm(
        home.path(),
        project.path(),
        &["--output", "json", "secret", "status", "ci.token"],
    )
    .env(
        "MM_FILE_PROVIDER_TEST_TOKEN",
        "first-line-value\nsecond-line-value",
    )
    .output()?;
    let stdout = String::from_utf8(output.stdout.clone())?;
    assert!(!stdout.contains("first-line-value"), "{stdout}");
    let status = json_status(&output)?;
    assert_eq!(status["secrets"][0]["status"], "invalid");
    let reasons = status["diagnostics"].to_string();
    assert!(
        reasons.contains(
            "environment variable MM_FILE_PROVIDER_TEST_TOKEN contains a newline; use the file provider for multi-line values"
        ),
        "{reasons}"
    );
    Ok(())
}
