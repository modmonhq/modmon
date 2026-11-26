//! `mm secret status` says when each secret's revision was written and who
//! wrote it, without its value; a provider that reports neither shows `—`.

mod temp_dirs;

use std::fs;
use std::path::Path;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const VALUE: &str = "SET_BY_SENTINEL_VALUE";
const ENV_SECRET: &str =
    "  stripe.key:\n    storage:\n      provider: env\n      location: MM_SET_BY_TEST_TOKEN\n";

fn project() -> TestResult<tempfile::TempDir> {
    let project = temp_dirs::tempdir()?;
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal");
    for file in [".config/mm.yaml", "compose.yaml", "api/.env.example"] {
        let destination = project.path().join(file);
        fs::create_dir_all(destination.parent().ok_or("no parent")?)?;
        fs::copy(fixture.join(file), destination)?;
    }
    let config = project.path().join(".config/mm.yaml");
    let text = fs::read_to_string(&config)?
        .replace("environments:\n", &format!("{ENV_SECRET}environments:\n"));
    fs::write(config, text)?;
    Ok(project)
}

fn mm(home: &Path, project: &Path, user: &str, arguments: &[&str]) -> assert_cmd::Command {
    let mut command = temp_dirs::mm();
    command
        .env("HOME", home)
        .env("USER", user)
        .env("MM_SET_BY_TEST_TOKEN", VALUE)
        .env_remove("MM_HOME")
        .env_remove("LOGNAME")
        .arg("--root")
        .arg(project)
        .args(arguments);
    command
}

fn set(home: &Path, project: &Path, user: &str, verb: &str) -> TestResult {
    let output = mm(
        home,
        project,
        user,
        &["secret", verb, "database.password", "--stdin"],
    )
    .write_stdin(format!("{VALUE}-{user}\n"))
    .output()?;
    assert!(output.status.success(), "{output:?}");
    Ok(())
}

fn status(home: &Path, project: &Path) -> TestResult<serde_json::Value> {
    let output = mm(
        home,
        project,
        "viewer",
        &["--output", "json", "secret", "status"],
    )
    .output()?;
    let stdout = String::from_utf8(output.stdout)?;
    assert!(!stdout.contains(VALUE), "{stdout}");
    Ok(serde_json::from_str(&stdout)?)
}

fn row<'a>(status: &'a serde_json::Value, key: &str) -> TestResult<&'a serde_json::Value> {
    status["secrets"]
        .as_array()
        .and_then(|secrets| secrets.iter().find(|secret| secret["key"] == key))
        .ok_or_else(|| format!("no row for {key} in {status}").into())
}

fn now_ms() -> TestResult<i64> {
    Ok(i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    )?)
}

#[test]
fn status_shows_when_and_by_whom_each_revision_was_set() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let project = project()?;
    let (home, project) = (home.path(), project.path());
    mm(home, project, "alex", &["secret", "init", "age"])
        .assert()
        .success();

    let unset = status(home, project)?;
    let database = row(&unset, "database.password")?;
    assert!(database.get("set_at_unix_ms").is_none(), "{database}");
    assert!(database.get("set_by").is_none(), "{database}");

    let before = now_ms()?;
    set(home, project, "alex", "set")?;
    let first = status(home, project)?;
    let database = row(&first, "database.password")?;
    assert_eq!(database["set_by"], "alex");
    let set_at = database["set_at_unix_ms"]
        .as_i64()
        .ok_or("no set_at_unix_ms")?;
    assert!(set_at >= before && set_at <= now_ms()?, "{database}");
    let first_revision = database["revision"].clone();

    set(home, project, "sam", "set")?;
    let second = status(home, project)?;
    let database = row(&second, "database.password")?;
    assert_eq!(
        database["set_by"], "sam",
        "the earlier writer does not stick"
    );
    assert_ne!(database["revision"], first_revision);

    let env = row(&second, "stripe.key")?;
    assert_eq!(env["status"], "set");
    assert!(env.get("set_at_unix_ms").is_none(), "{env}");
    assert!(env.get("set_by").is_none(), "{env}");

    let output = mm(home, project, "viewer", &["secret", "status"]).output()?;
    assert!(output.status.success(), "{output:?}");
    let table = String::from_utf8(output.stdout)?;
    assert!(!table.contains(VALUE), "{table}");
    let header = table
        .lines()
        .find(|line| line.trim_start().starts_with("KEY"))
        .ok_or("no header")?;
    for column in ["PROVIDER", "STATUS", "SET", "BY", "LOCATION"] {
        assert!(header.contains(column), "{header}");
    }
    let database = table
        .lines()
        .find(|line| line.trim_start().starts_with("database.password"))
        .ok_or("no database row")?;
    assert!(database.contains("just now ("), "{database}");
    assert!(database.contains(" sam "), "{database}");
    assert!(database.chars().count() <= 100, "{database}");
    let env = table
        .lines()
        .find(|line| line.trim_start().starts_with("stripe.key"))
        .ok_or("no env row")?;
    assert_eq!(env.matches('—').count(), 2, "{env}");
    Ok(())
}

#[test]
fn a_long_writer_is_cut_and_the_key_is_kept() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let project = project()?;
    let (home, project) = (home.path(), project.path());
    mm(home, project, "alex", &["secret", "init", "age"])
        .assert()
        .success();
    let writer = "a-very-long-login-name-for-a-shared-build-account";
    set(home, project, writer, "set")?;
    assert_eq!(
        row(&status(home, project)?, "database.password")?["set_by"],
        writer,
        "JSON keeps the whole name"
    );
    let output = mm(home, project, "viewer", &["secret", "status"]).output()?;
    let table = String::from_utf8(output.stdout)?;
    let database = table
        .lines()
        .find(|line| line.trim_start().starts_with("database.password"))
        .ok_or("no database row")?;
    assert!(!database.contains(writer), "{database}");
    assert!(database.contains("a-very-long-login-n…"), "{database}");
    Ok(())
}

mod age_store {
    use mm::adapters::secrets::AgeSecretStore;
    use mm::app::ports::{
        SecretBatchReplacer, SecretCandidateLifecycle, SecretCreator, SecretMetadataInspector,
        SecretReplacer,
    };
    use mm::domain::secret::{
        BundledSecret, SecretMaterial, SecretMetadata, SecretReference, SecretReplacement,
        SecretScope,
    };

    use super::{TestResult, temp_dirs};

    fn material(value: &str) -> TestResult<SecretMaterial> {
        Ok(SecretMaterial::new(value.to_owned())?)
    }

    async fn inspect(
        store: &AgeSecretStore,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> TestResult<SecretMetadata> {
        Ok(store.inspect(scope, reference).await?)
    }

    #[tokio::test]
    async fn every_write_records_its_writer_and_a_candidate_waits_for_promotion() -> TestResult {
        let directory = temp_dirs::tempdir()?;
        let store = AgeSecretStore::new(
            directory.path().join("age.key"),
            directory.path().join("project.age"),
        );
        store.initialize()?;
        let scope = SecretScope::new("local");
        let reference: SecretReference = "age://app/password".parse()?;

        store
            .create(&scope, &reference, material("one")?, 1_000, Some("alex"))
            .await?;
        let created = inspect(&store, &scope, &reference).await?;
        assert_eq!(created.set_by.as_deref(), Some("alex"));
        assert_eq!(created.created_at_unix_ms, Some(1_000));

        store
            .replace(
                &scope,
                &reference,
                material("two")?,
                created.expected_state(),
                2_000,
                Some("sam"),
            )
            .await?;
        let replaced = inspect(&store, &scope, &reference).await?;
        assert_eq!(replaced.set_by.as_deref(), Some("sam"));
        assert_eq!(replaced.created_at_unix_ms, Some(2_000));

        store
            .stage(
                &scope,
                &reference,
                material("three")?,
                replaced.expected_state(),
                3_000,
                Some("agent:claude (alex)"),
            )
            .await?;
        let staged = inspect(&store, &scope, &reference).await?;
        assert!(staged.candidate_revision.is_some());
        assert_eq!(
            (staged.set_by.as_deref(), staged.created_at_unix_ms),
            (Some("sam"), Some(2_000)),
            "a staged candidate leaves the active writer and time"
        );
        store
            .promote(&scope, &reference, staged.expected_state(), 4_000)
            .await?;
        let promoted = inspect(&store, &scope, &reference).await?;
        assert_eq!(promoted.set_by.as_deref(), Some("agent:claude (alex)"));
        assert_eq!(promoted.created_at_unix_ms, Some(3_000));

        store
            .replace_batch(
                vec![SecretReplacement {
                    scope: scope.clone(),
                    reference: reference.clone(),
                    material: material("four")?,
                    expected: promoted.expected_state(),
                }],
                5_000,
                None,
            )
            .await?;
        let anonymous = inspect(&store, &scope, &reference).await?;
        assert_eq!(
            anonymous.set_by, None,
            "a write without a writer never guesses"
        );
        assert_eq!(anonymous.created_at_unix_ms, Some(5_000));
        Ok(())
    }

    #[tokio::test]
    async fn an_imported_bundle_keeps_the_original_writer_and_time() -> TestResult {
        let directory = temp_dirs::tempdir()?;
        let host = AgeSecretStore::new(
            directory.path().join("age.key"),
            directory.path().join("host.age"),
        );
        host.initialize()?;
        let scope = SecretScope::new("production");
        let written: SecretReference = "age://written".parse()?;
        let older: SecretReference = "age://older".parse()?;
        host.mirror(
            &scope,
            vec![
                BundledSecret {
                    reference: written.clone(),
                    revision: "revision-1".to_owned(),
                    material: material("value")?,
                    set_at_unix_ms: Some(1_000),
                    set_by: Some("alex".to_owned()),
                },
                BundledSecret {
                    reference: older.clone(),
                    revision: "revision-2".to_owned(),
                    material: material("value")?,
                    set_at_unix_ms: None,
                    set_by: None,
                },
            ],
            9_000,
        )?;
        let written = inspect(&host, &scope, &written).await?;
        assert_eq!(
            (written.set_by.as_deref(), written.created_at_unix_ms),
            (Some("alex"), Some(1_000))
        );
        let older = inspect(&host, &scope, &older).await?;
        assert_eq!(
            (older.set_by, older.created_at_unix_ms),
            (None, Some(9_000))
        );
        Ok(())
    }
}
