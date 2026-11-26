use std::fs;
use std::os::unix::fs::PermissionsExt;

use zeroize::Zeroizing;

use super::*;
use crate::adapters::secrets::seal_secret_bundle;
use crate::domain::secret::{BundledDeployKey, BundledSecret, SecretMaterial};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const PROJECT: &str = "019c527b-6242-7b43-bfd4-bd60e27c1de3";
const BREAK_GLASS: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJZYcbK/sLQnEppB74bVUmUQnMUahuREnHUmhtFYGcf7 ops@desk";

fn project() -> Result<ProjectId, uuid::Error> {
    Ok(ProjectId(PROJECT.parse()?))
}

fn config(root: &Path) -> Result<DaemonConfig, Box<dyn std::error::Error>> {
    for directory in ["etc", "state"] {
        fs::create_dir_all(root.join(directory))?;
        fs::set_permissions(root.join(directory), fs::Permissions::from_mode(0o700))?;
    }
    let host = root.join("etc/host.yaml");
    fs::write(
        &host,
        format!(
            "schema-version: 1\nbreak-glass:\n  - name: ops\n    key: {BREAK_GLASS}\nprojects:\n  {PROJECT}:\n    name: shop\n    origin: /srv/git/shop.git\n    environments: [integration]\n"
        ),
    )?;
    fs::set_permissions(&host, fs::Permissions::from_mode(0o600))?;
    Ok(serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "socket_path": root.join("run/mm.sock"),
        "state_root": root.join("state"),
        "mm_home": root.join("home"),
        "socket_uid": 0,
        "socket_gid": 0,
        "host_policy_path": host,
        "relay_uid": 991,
        "authorized_keys_path": root.join("ssh/authorized_keys"),
        "projects_root": root.join("projects"),
    }))?)
}

fn key_pair(root: &Path) -> Result<(Vec<u8>, String), Box<dyn std::error::Error>> {
    let path = root.join("operator-deploy-key");
    let status = std::process::Command::new("ssh-keygen")
        .args([
            "-q",
            "-t",
            "ed25519",
            "-N",
            "",
            "-C",
            "mm-deploy-integration",
            "-f",
        ])
        .arg(&path)
        .status()?;
    assert!(status.success());
    Ok((
        fs::read(&path)?,
        fs::read_to_string(path.with_extension("pub"))?
            .trim()
            .to_owned(),
    ))
}

fn bundle(
    environment: &str,
    revision: &str,
    deploy_key: Option<(&[u8], &str)>,
) -> Result<SecretBundle, Box<dyn std::error::Error>> {
    Ok(SecretBundle {
        project_id: project()?,
        environment: environment.to_owned(),
        secrets: vec![BundledSecret {
            reference: "age://database-password".parse()?,
            revision: revision.to_owned(),
            material: SecretMaterial::new(format!("value-{revision}"))?,
            set_at_unix_ms: None,
            set_by: None,
        }],
        deploy_key: deploy_key.map(|(private, public)| BundledDeployKey {
            private_key: Zeroizing::new(private.to_vec()),
            public_key: public.to_owned(),
        }),
    })
}

#[test]
fn imports_secrets_with_their_revisions_and_installs_the_deploy_key() -> TestResult {
    let temp = crate::test_support::tempdir()?;
    let config = config(temp.path())?;
    let project = project()?;
    let recipient = host_secret_recipient(&config, project, "integration")?;
    assert_eq!(
        host_secret_recipient(&config, project, "integration")?,
        recipient
    );
    let (private, public) = key_pair(temp.path())?;

    let sealed = seal_secret_bundle(
        &recipient,
        &bundle("integration", "r1", Some((&private, &public)))?,
    )?;
    let summary = import_host_secrets(&config, project, "integration", &sealed, 1)?;
    let json = serde_json::to_value(&summary)?;
    assert_eq!(
        json["secrets"]["age://database-password"],
        serde_json::json!({"revision": "r1", "changed": true})
    );
    assert_eq!(
        json["deploy-key"],
        serde_json::json!({"public-key": public, "changed": true})
    );
    assert!(!json.to_string().contains("value-r1"));
    let access = HostAccessStore::new(&config);
    assert_eq!(access.ensure_deploy_key(project)?, public);

    let again = import_host_secrets(&config, project, "integration", &sealed, 2)?;
    assert!(again.secrets.values().all(|entry| !entry.changed));
    assert_eq!(again.deploy_key.map(|key| key.changed), Some(false));

    let next = seal_secret_bundle(&recipient, &bundle("integration", "r2", None)?)?;
    let summary = import_host_secrets(&config, project, "integration", &next, 3)?;
    assert!(summary.secrets["age://database-password"].changed);
    assert_eq!(summary.deploy_key, None);
    Ok(())
}

#[test]
fn rejects_bundles_for_another_scope_or_an_unenrolled_environment() -> TestResult {
    let temp = crate::test_support::tempdir()?;
    let config = config(temp.path())?;
    let project = project()?;
    let recipient = host_secret_recipient(&config, project, "integration")?;

    let production = seal_secret_bundle(&recipient, &bundle("production", "r1", None)?)?;
    let error = import_host_secrets(&config, project, "integration", &production, 1)
        .err()
        .ok_or("a production bundle was imported into integration")?;
    assert!(error.to_string().contains("production"), "{error}");
    assert!(import_host_secrets(&config, project, "production", &production, 1).is_err());

    let mut other = bundle("integration", "r1", None)?;
    other.project_id = ProjectId(uuid::Uuid::nil());
    let other = seal_secret_bundle(&recipient, &other)?;
    assert!(import_host_secrets(&config, project, "integration", &other, 1).is_err());

    let stranger = age::x25519::Identity::generate().to_public().to_string();
    let foreign = seal_secret_bundle(&stranger, &bundle("integration", "r1", None)?)?;
    assert!(import_host_secrets(&config, project, "integration", &foreign, 1).is_err());
    assert!(host_secret_recipient(&config, project, "../etc").is_err());
    Ok(())
}
