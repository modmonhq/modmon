//! `mm secret generate --missing` takes the secrets the selected environment
//! uses, and a provider with no connection from here is skipped.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use mm::adapters::clock::FixedClock;
use mm::adapters::config::ConfigLoader;
use mm::adapters::environment::FilesystemEnvironmentArtifactStore;
use mm::adapters::secrets::{
    AgeSecretStore, EnvironmentSecretStore, SecretProviderRegistry, StorageProviderRegistration,
    age_storage_descriptor, environment_storage_descriptor, openbao_storage_descriptor,
};
use mm::app::ports::{
    SecretBatchReplacer, SecretCandidateLifecycle, SecretCreator, SecretDeliveryCapabilityCatalog,
    SecretMaterialResolver, SecretMetadataInspector, SecretRemover, SecretReplacer,
    SecretStorageCapabilityCatalog,
};
use mm::app::{SecretManager, SecretManagerDependencies};
use mm::domain::error::MmError;
use mm::domain::secret::{
    SecretExpectedState, SecretGenerationReport, SecretGenerationSelection, SecretGenerationStatus,
    SecretMaterial, SecretMetadata, SecretReference, SecretResolution, SecretScope, SecretStatus,
    SecretWriteResult,
};
use mm::domain::secret_backend::ProviderInstanceName;

mod temp_dirs;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const CONFIG: &str = r"schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: per-environment
default-environment: local
target: local
runtime:
  driver: docker-compose
stacks:
  app:
    files: [compose.yaml]
modules:
  api:
    path: api
    services:
      web:
        stack: app
        secret-files:
          api-token: {secret: api.token, environments: [staging]}
providers:
  team-secrets:
    driver: openbao
    scope: project
    mode: external
    address: https://bao.example.test:8200
    storage-mount: mm-kv
    automation-auth-mount: mm-approle
    trust-bundle: bao-ca.pem
    expected-trust-sha256: sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
secrets:
  local.database:
    storage: {provider: age, location: local/database}
    generate: {preset: password, length: 32}
  session.key:
    storage: {provider: age, location: session-key}
    generate: {preset: token, length: 32}
  api.token:
    storage: {provider: age, location: api-token}
    generate: {preset: token, length: 32}
  staging.database:
    storage: {provider: team-secrets, location: database/password}
    generate: {preset: password, length: 32}
  payment.key:
    storage: {provider: age, location: payment-key}
    input: {prompt: Payment provider key}
bindings:
  SESSION.KEY: {secret: session.key}
environments:
  local:
    bindings:
      DB.PASSWORD: {secret: local.database}
  staging:
    bindings:
      DB.PASSWORD: {secret: staging.database}
";

#[tokio::test]
async fn missing_takes_only_the_environments_secrets_and_skips_an_unconnected_provider()
-> TestResult {
    let project = project()?;
    let home = temp_dirs::tempdir()?;
    let age = Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    ));
    age.initialize()?;
    let unconnected = Arc::new(Unconnected::default());
    let manager = manager(Arc::clone(&age), Arc::clone(&unconnected))?;

    let local = manager
        .generate(project.path(), None, &SecretGenerationSelection::Missing)
        .await?;
    assert_eq!(local.environment, "local");
    assert_eq!(
        statuses(&local),
        [
            ("local.database", SecretGenerationStatus::Generated),
            ("session.key", SecretGenerationStatus::Generated),
        ]
    );
    assert_eq!(unconnected.calls(), 0, "local never asks team-secrets");

    let staging = manager
        .generate(
            project.path(),
            Some("staging"),
            &SecretGenerationSelection::Missing,
        )
        .await?;
    assert_eq!(
        statuses(&staging),
        [
            ("api.token", SecretGenerationStatus::Generated),
            ("session.key", SecretGenerationStatus::Generated),
            ("staging.database", SecretGenerationStatus::Skipped),
        ]
    );
    let skipped = &staging.secrets[2];
    assert_eq!(skipped.provider, "team-secrets");
    assert_eq!(skipped.revision, None);
    assert!(
        skipped
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("no connection to secret provider")),
        "{skipped:?}"
    );
    assert_eq!(unconnected.calls(), 1);
    let local_database: SecretReference = "age://local/database".parse()?;
    assert_eq!(
        age.inspect(&SecretScope::new("staging"), &local_database)
            .await?
            .status,
        SecretStatus::Unset,
        "staging leaves the local secret alone"
    );

    let error = manager
        .generate(
            project.path(),
            Some("staging"),
            &SecretGenerationSelection::Named("staging.database".to_owned()),
        )
        .await
        .err()
        .ok_or("a named secret its provider cannot reach fails")?;
    assert_eq!(error.code(), "MM_ENVIRONMENT");
    Ok(())
}

#[tokio::test]
async fn all_takes_every_generated_secret_as_before() -> TestResult {
    let project = project()?;
    let home = temp_dirs::tempdir()?;
    let age = Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    ));
    age.initialize()?;
    let unconnected = Arc::new(Unconnected::default());
    let manager = manager(age, Arc::clone(&unconnected))?;

    let all = manager
        .generate(project.path(), None, &SecretGenerationSelection::All)
        .await?;
    assert_eq!(
        statuses(&all),
        [
            ("api.token", SecretGenerationStatus::Generated),
            ("local.database", SecretGenerationStatus::Generated),
            ("session.key", SecretGenerationStatus::Generated),
            ("staging.database", SecretGenerationStatus::Skipped),
        ]
    );
    let again = manager
        .generate(project.path(), None, &SecretGenerationSelection::All)
        .await?;
    assert!(
        again.secrets[..3]
            .iter()
            .all(|secret| secret.status == SecretGenerationStatus::AlreadySet)
    );
    assert_eq!(unconnected.calls(), 2);
    Ok(())
}

#[test]
fn the_cli_generates_per_environment_and_lists_what_it_skipped() -> TestResult {
    let project = project()?;
    let home = temp_dirs::tempdir()?;
    let mm = |arguments: &[&str]| {
        let mut command = temp_dirs::mm();
        command.args([
            "--root",
            &project.path().to_string_lossy(),
            "--mm-home",
            &home.path().to_string_lossy(),
        ]);
        command.args(arguments);
        command
    };
    mm(&["secret", "init", "age"]).assert().success();

    let local = generation(
        mm(&["--output", "json", "secret", "generate", "--missing"])
            .assert()
            .success(),
    )?;
    assert_eq!(
        statuses(&local),
        [
            ("local.database", SecretGenerationStatus::Generated),
            ("session.key", SecretGenerationStatus::Generated),
        ]
    );
    assert!(
        !mentions(home.path(), "openbao-connections")?,
        "a local run never opens team-secrets' connection"
    );

    let staging = generation(
        mm(&[
            "-e",
            "staging",
            "--output",
            "json",
            "secret",
            "generate",
            "--missing",
        ])
        .assert()
        .success(),
    )?;
    assert_eq!(
        statuses(&staging),
        [
            ("api.token", SecretGenerationStatus::Generated),
            ("session.key", SecretGenerationStatus::Generated),
            ("staging.database", SecretGenerationStatus::Skipped),
        ]
    );
    assert!(mentions(home.path(), "openbao-connections")?);

    let text = mm(&["-e", "staging", "secret", "generate", "--missing"])
        .assert()
        .success();
    let text = String::from_utf8_lossy(&text.get_output().stdout).into_owned();
    assert!(text.contains("Skipped staging.database: no connection to secret provider 'team-secrets' for environment 'staging' here"), "{text}");
    assert!(
        text.contains("`mm secret provider add team-secrets`"),
        "{text}"
    );

    let all = generation(
        mm(&[
            "-e", "staging", "--output", "json", "secret", "generate", "--all",
        ])
        .assert()
        .success(),
    )?;
    assert_eq!(
        statuses(&all),
        [
            ("api.token", SecretGenerationStatus::AlreadySet),
            ("local.database", SecretGenerationStatus::Generated),
            ("session.key", SecretGenerationStatus::AlreadySet),
            ("staging.database", SecretGenerationStatus::Skipped),
        ]
    );

    mm(&["-e", "staging", "secret", "generate", "staging.database"])
        .assert()
        .code(2)
        .stderr(predicates::str::contains("MM_ENVIRONMENT"));
    mm(&["secret", "generate", "--missing", "--all"])
        .assert()
        .code(2);
    Ok(())
}

fn project() -> TestResult<tempfile::TempDir> {
    let project = temp_dirs::tempdir()?;
    fs::create_dir_all(project.path().join(".config"))?;
    fs::create_dir_all(project.path().join("api"))?;
    fs::write(project.path().join(".config/mm.yaml"), CONFIG)?;
    fs::write(
        project.path().join("compose.yaml"),
        "services:\n  web:\n    image: docker.io/library/alpine:3.22\n",
    )?;
    Ok(project)
}

fn statuses(report: &SecretGenerationReport) -> Vec<(&str, SecretGenerationStatus)> {
    report
        .secrets
        .iter()
        .map(|secret| (secret.name.as_str(), secret.status))
        .collect()
}

fn generation(assert: assert_cmd::assert::Assert) -> TestResult<SecretGenerationReport> {
    Ok(serde_json::from_slice(&assert.get_output().stdout)?)
}

fn mentions(root: &Path, name: &str) -> TestResult<bool> {
    let mut pending: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.to_string_lossy().contains(name) {
                return Ok(true);
            }
            if path.is_dir() {
                pending.push(path);
            }
        }
    }
    Ok(false)
}

fn manager(age: Arc<AgeSecretStore>, unconnected: Arc<Unconnected>) -> TestResult<SecretManager> {
    let registry = Arc::new(SecretProviderRegistry::new(vec![
        age_registration(age)?,
        environment_registration()?,
        unconnected_registration(unconnected)?,
    ])?);
    let inspector: Arc<dyn SecretMetadataInspector> = registry.clone();
    let creator: Arc<dyn SecretCreator> = registry.clone();
    let replacer: Arc<dyn SecretReplacer> = registry.clone();
    let batch_replacer: Arc<dyn SecretBatchReplacer> = registry.clone();
    let remover: Arc<dyn SecretRemover> = registry.clone();
    let storage_capabilities: Arc<dyn SecretStorageCapabilityCatalog> = registry;
    let delivery_capabilities: Arc<dyn SecretDeliveryCapabilityCatalog> =
        Arc::new(FilesystemEnvironmentArtifactStore::new(PathBuf::from(
            "/tmp/mm-test-secret-generation-unused",
        ))?);
    Ok(SecretManager::new(SecretManagerDependencies {
        documents: Arc::new(ConfigLoader),
        inspector,
        creator,
        replacer,
        batch_replacer,
        remover,
        storage_capabilities,
        delivery_capabilities,
        clock: Arc::new(FixedClock { unix_ms: 1_000 }),
    }))
}

fn age_registration(age: Arc<AgeSecretStore>) -> TestResult<StorageProviderRegistration> {
    let inspector: Arc<dyn SecretMetadataInspector> = age.clone();
    let resolver: Arc<dyn SecretMaterialResolver> = age.clone();
    let creator: Arc<dyn SecretCreator> = age.clone();
    let replacer: Arc<dyn SecretReplacer> = age.clone();
    let batch_replacer: Arc<dyn SecretBatchReplacer> = age.clone();
    let candidates: Arc<dyn SecretCandidateLifecycle> = age.clone();
    let remover: Arc<dyn SecretRemover> = age;
    Ok(StorageProviderRegistration {
        descriptor: age_storage_descriptor()?,
        inspector,
        resolver,
        creator: Some(creator),
        replacer: Some(replacer),
        batch_replacer: Some(batch_replacer),
        candidates: Some(candidates),
        remover: Some(remover),
    })
}

fn environment_registration() -> TestResult<StorageProviderRegistration> {
    let environment = Arc::new(EnvironmentSecretStore::new([17_u8; 32]));
    let inspector: Arc<dyn SecretMetadataInspector> = environment.clone();
    let resolver: Arc<dyn SecretMaterialResolver> = environment;
    Ok(StorageProviderRegistration {
        descriptor: environment_storage_descriptor()?,
        inspector,
        resolver,
        creator: None,
        replacer: None,
        batch_replacer: None,
        candidates: None,
        remover: None,
    })
}

fn unconnected_registration(
    unconnected: Arc<Unconnected>,
) -> TestResult<StorageProviderRegistration> {
    let inspector: Arc<dyn SecretMetadataInspector> = unconnected.clone();
    let resolver: Arc<dyn SecretMaterialResolver> = unconnected.clone();
    let creator: Arc<dyn SecretCreator> = unconnected.clone();
    let replacer: Arc<dyn SecretReplacer> = unconnected.clone();
    let remover: Arc<dyn SecretRemover> = unconnected;
    Ok(StorageProviderRegistration {
        descriptor: openbao_storage_descriptor(&ProviderInstanceName::new("team-secrets")?)?,
        inspector,
        resolver,
        creator: Some(creator),
        replacer: Some(replacer),
        batch_replacer: None,
        candidates: None,
        remover: Some(remover),
    })
}

/// A provider this machine has no connection to, counting every call.
#[derive(Default)]
struct Unconnected {
    calls: AtomicUsize,
}

impl Unconnected {
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn refuse<T>(&self, scope: &SecretScope) -> Result<T, MmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(MmError::Environment {
            message: format!(
                "no connection to secret provider 'team-secrets' for environment '{}' here",
                scope.environment()
            ),
        })
    }
}

#[async_trait]
impl SecretMetadataInspector for Unconnected {
    async fn inspect(
        &self,
        scope: &SecretScope,
        _: &SecretReference,
    ) -> Result<SecretMetadata, MmError> {
        self.refuse(scope)
    }
}

#[async_trait]
impl SecretMaterialResolver for Unconnected {
    async fn resolve(
        &self,
        scope: &SecretScope,
        _: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        self.refuse(scope)
    }
}

#[async_trait]
impl SecretCreator for Unconnected {
    async fn create(
        &self,
        scope: &SecretScope,
        _: &SecretReference,
        _: SecretMaterial,
        _: i64,
        _: Option<&str>,
    ) -> Result<SecretWriteResult, MmError> {
        self.refuse(scope)
    }
}

#[async_trait]
impl SecretReplacer for Unconnected {
    async fn replace(
        &self,
        scope: &SecretScope,
        _: &SecretReference,
        _: SecretMaterial,
        _: SecretExpectedState,
        _: i64,
        _: Option<&str>,
    ) -> Result<String, MmError> {
        self.refuse(scope)
    }
}

#[async_trait]
impl SecretRemover for Unconnected {
    async fn unset(
        &self,
        scope: &SecretScope,
        _: &SecretReference,
        _: SecretExpectedState,
    ) -> Result<bool, MmError> {
        self.refuse(scope)
    }
}
