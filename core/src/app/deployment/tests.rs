use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use uuid::Uuid;

use super::*;
use crate::domain::ingress::{
    CanonicalMiddleware, IngressCertificateReference, IngressRouteOwnership, IngressRouteProvenance,
};
use crate::domain::stack::{PreparedStackArtifact, StackArtifactManifest};
use crate::domain::target::TargetId;
use crate::domain::workspace::ProjectId;

struct FakeLocalCa {
    failure: Option<&'static str>,
    reconciled: Mutex<Option<std::collections::BTreeSet<String>>>,
}

impl FakeLocalCa {
    fn new(failure: Option<&'static str>) -> Self {
        Self {
            failure,
            reconciled: Mutex::new(None),
        }
    }

    fn check(&self, operation: &'static str) -> Result<(), MmError> {
        if self.failure == Some(operation) {
            Err(MmError::State {
                message: format!("fake {operation} failure"),
            })
        } else {
            Ok(())
        }
    }

    fn unexpected<T>(&self, operation: &'static str) -> Result<T, MmError> {
        Err(MmError::State {
            message: format!("unexpected fake {operation} call"),
        })
    }
}

impl LocalCaStore for FakeLocalCa {
    fn status(&self) -> Result<crate::domain::ca::LocalCaStatus, MmError> {
        self.check("status")?;
        Ok(crate::domain::ca::LocalCaStatus {
            installed: true,
            fingerprint: Some("fingerprint".to_owned()),
            generation: 1,
            issued_hosts: [
                "mm.test".to_owned(),
                "mail.mm.test".to_owned(),
                "mm.registry.test".to_owned(),
                "old.project.test".to_owned(),
            ]
            .into_iter()
            .collect(),
            key_permissions_valid: true,
            trust_disabled: false,
            rotation_pending: false,
            leaves: Vec::new(),
            trust: None,
        })
    }

    fn install(&self) -> Result<crate::domain::ca::LocalCaStatus, MmError> {
        self.unexpected("install")
    }

    fn rotate(&self) -> Result<crate::domain::ca::LocalCaStatus, MmError> {
        self.unexpected("rotate")
    }

    fn finish_rotation(&self) -> Result<crate::domain::ca::LocalCaStatus, MmError> {
        self.unexpected("finish rotation")
    }

    fn rollback_rotation(&self) -> Result<crate::domain::ca::LocalCaStatus, MmError> {
        self.unexpected("rollback rotation")
    }

    fn rotation_previous_fingerprint(&self) -> Result<Option<String>, MmError> {
        self.unexpected("rotation fingerprint")
    }

    fn uninstall(&self) -> Result<crate::domain::ca::LocalCaStatus, MmError> {
        self.unexpected("uninstall")
    }

    fn issue(
        &self,
        _hosts: &std::collections::BTreeSet<String>,
    ) -> Result<Vec<crate::domain::ca::IssuedCertificate>, MmError> {
        self.unexpected("issue")
    }

    fn certificate_pem(&self, _host: &str) -> Result<String, MmError> {
        self.unexpected("certificate pem")
    }

    fn key_pem(&self, _host: &str) -> Result<String, MmError> {
        self.unexpected("key pem")
    }

    fn ca_certificate_pem(&self) -> Result<String, MmError> {
        self.unexpected("ca certificate pem")
    }

    fn set_trust_disabled(
        &self,
        _disabled: bool,
    ) -> Result<crate::domain::ca::LocalCaStatus, MmError> {
        self.unexpected("set trust disabled")
    }

    fn reconcile(
        &self,
        hosts: &std::collections::BTreeSet<String>,
    ) -> Result<Vec<crate::domain::ca::IssuedCertificate>, MmError> {
        self.check("reconcile")?;
        let mut reconciled = self.reconciled.lock().map_err(|_| MmError::State {
            message: "fake reconciled hosts lock is poisoned".to_owned(),
        })?;
        *reconciled = Some(hosts.clone());
        Ok(Vec::new())
    }

    fn certificate_path(&self, host: &str) -> Result<PathBuf, MmError> {
        self.check("certificate path")?;
        Ok(PathBuf::from(format!("certificates/{host}.pem")))
    }

    fn key_path(&self, host: &str) -> Result<PathBuf, MmError> {
        self.check("key path")?;
        Ok(PathBuf::from(format!("certificates/{host}-key.pem")))
    }
}

fn claim(project: u128, environment: &str, host: &str, path: &str) -> IngressRouteClaim {
    let project_id = ProjectId(Uuid::from_u128(project));
    IngressRouteClaim {
        target: "local".to_owned(),
        target_id: TargetId(Uuid::from_u128(10)),
        project_id,
        project_name: format!("project-{project}"),
        environment: environment.to_owned(),
        stack: "app".to_owned(),
        route: "web".to_owned(),
        host: host.to_owned(),
        path_prefix: path.to_owned(),
        service: "web".to_owned(),
        port: 8080,
        tls: true,
        provenance: IngressRouteProvenance::ExplicitConfig,
        certificate_reference: IngressCertificateReference::LocalCa {
            host: host.to_owned(),
        },
        ownership: IngressRouteOwnership::Project {
            project_id,
            environment: environment.to_owned(),
            stack: "app".to_owned(),
            service: "web".to_owned(),
        },
        priority: 10,
        router_name: format!("router-{project}"),
        backend_alias: format!("backend-{project}"),
        artifact_hash: format!("hash-{project}"),
        middlewares: Vec::new(),
    }
}

fn artifact(stack: &str) -> PreparedStackArtifact {
    PreparedStackArtifact {
        root: PathBuf::from(format!("artifacts/{stack}")),
        manifest_path: PathBuf::from(format!("artifacts/{stack}/manifest.json")),
        generated_overlay_path: PathBuf::from(format!("artifacts/{stack}/overlay.yaml")),
        compose_files: Vec::new(),
        manifest: StackArtifactManifest {
            project_id: ProjectId(Uuid::from_u128(1)),
            project_name: "project".to_owned(),
            target: "local".to_owned(),
            target_id: TargetId(Uuid::from_u128(10)),
            environment: "development".to_owned(),
            stack: stack.to_owned(),
            compose_project_name: format!("project-{stack}"),
            user_files: Vec::new(),
            generated_overlay_digest: "overlay".to_owned(),
            services: Vec::new(),
            routes: Vec::new(),
            middleware_inputs: Vec::new(),
            preserved_volumes: Vec::new(),
            security: Vec::new(),
            low_ports: Vec::new(),
            artifact_hash: format!("artifact-{stack}"),
        },
    }
}

fn gate(address_suffix: &str) -> CanonicalMiddleware {
    CanonicalMiddleware::ForwardAuth {
        address: format!("http://mm-control-host:42831/_mm/gate/forward/{address_suffix}"),
        trust_forward_header: true,
        auth_response_headers: vec!["X-MM-User".to_owned()],
        add_auth_cookies_to_response: Vec::new(),
    }
}

fn replace_path() -> CanonicalMiddleware {
    CanonicalMiddleware::ReplacePathRegex {
        regex: "^/old".to_owned(),
        replacement: "/new".to_owned(),
    }
}

fn error_message(error: MmError) -> String {
    match error {
        MmError::Security { message }
        | MmError::Environment { message }
        | MmError::State { message } => message,
        other => other.to_string(),
    }
}

#[test]
fn local_host_selection_is_restricted_to_local_tls_routes_and_service_hosts() {
    assert!(is_service_dashboard_host("mm.test"));
    assert!(is_service_dashboard_host("mail.mm.test"));
    assert!(is_service_dashboard_host("mm.registry.test"));
    assert!(!is_service_dashboard_host("dashboard.mm.test"));
    assert!(!is_service_dashboard_host("mm.nested.name.test"));
    assert!(!is_service_dashboard_host("app.example.test"));

    let mut local_tls = claim(1, "development", "app.test", "/");
    let mut no_tls = claim(1, "development", "plain.test", "/");
    no_tls.tls = false;
    let external = claim(1, "development", "app.example.com", "/");
    assert_eq!(
        local_tls_hosts(&[local_tls.clone(), no_tls, external]),
        ["app.test".to_owned()].into_iter().collect()
    );
    local_tls.host = "bad..test".to_owned();
    assert!(local_tls_hosts(&[local_tls]).is_empty());
}

#[test]
fn local_certificate_reconciliation_preserves_only_managed_service_hosts_and_propagates_errors()
-> Result<(), MmError> {
    let store = FakeLocalCa::new(None);
    let project = claim(1, "development", "app.test", "/");
    reconcile_local_certificates(&store, &[project])?;
    let reconciled = store
        .reconciled
        .lock()
        .map_err(|_| MmError::State {
            message: "fake reconciled hosts lock is poisoned".to_owned(),
        })?
        .clone();
    assert_eq!(
        reconciled,
        Some(
            [
                "app.test".to_owned(),
                "mail.mm.test".to_owned(),
                "mm.registry.test".to_owned(),
                "mm.test".to_owned(),
            ]
            .into_iter()
            .collect()
        )
    );
    assert!(reconcile_local_certificates(&FakeLocalCa::new(Some("status")), &[]).is_err());
    assert!(reconcile_local_certificates(&FakeLocalCa::new(Some("reconcile")), &[]).is_err());

    assert_eq!(
        local_certificate_paths(&store, "app.test")?,
        (
            PathBuf::from("certificates/app.test.pem"),
            PathBuf::from("certificates/app.test-key.pem")
        )
    );
    assert!(
        local_certificate_paths(&FakeLocalCa::new(Some("certificate path")), "app.test").is_err()
    );
    assert!(local_certificate_paths(&FakeLocalCa::new(Some("key path")), "app.test").is_err());
    Ok(())
}

#[test]
fn deployment_stack_identity_directories_order_and_uniqueness_are_enforced() -> Result<(), MmError>
{
    let mut directories = BTreeMap::from([
        ("worker".to_owned(), PathBuf::from("projects/worker")),
        ("app".to_owned(), PathBuf::from("projects/app")),
    ]);
    let stacks = deployment_stacks(
        ProjectId(Uuid::from_u128(1)),
        "project",
        TargetId(Uuid::from_u128(10)),
        "local",
        "development",
        vec![artifact("worker"), artifact("app")],
        directories.clone(),
    )?;
    assert_eq!(
        stacks
            .iter()
            .map(|stack| stack.stack.as_str())
            .collect::<Vec<_>>(),
        ["app", "worker"]
    );
    assert_eq!(stacks[0].project_directory, PathBuf::from("projects/app"));

    type Mutation = fn(&mut StackArtifactManifest);
    let mismatches: [Mutation; 5] = [
        |manifest| manifest.project_id = ProjectId(Uuid::from_u128(2)),
        |manifest| manifest.project_name = "other".to_owned(),
        |manifest| manifest.target_id = TargetId(Uuid::from_u128(11)),
        |manifest| manifest.target = "other".to_owned(),
        |manifest| manifest.environment = "production".to_owned(),
    ];
    for mutate in mismatches {
        let mut invalid = artifact("app");
        mutate(&mut invalid.manifest);
        let error = deployment_stacks(
            ProjectId(Uuid::from_u128(1)),
            "project",
            TargetId(Uuid::from_u128(10)),
            "local",
            "development",
            vec![invalid],
            BTreeMap::from([("app".to_owned(), PathBuf::from("projects/app"))]),
        )
        .err()
        .map(error_message)
        .unwrap_or_default();
        assert!(error.contains("does not match the deployment identity"));
    }

    directories.remove("app");
    let error = deployment_stacks(
        ProjectId(Uuid::from_u128(1)),
        "project",
        TargetId(Uuid::from_u128(10)),
        "local",
        "development",
        vec![artifact("app")],
        directories,
    )
    .err()
    .map(error_message)
    .unwrap_or_default();
    assert!(error.contains("no Compose project directory"));

    let error = deployment_stacks(
        ProjectId(Uuid::from_u128(1)),
        "project",
        TargetId(Uuid::from_u128(10)),
        "local",
        "development",
        vec![artifact("app")],
        BTreeMap::from([
            ("app".to_owned(), PathBuf::from("projects/app")),
            ("extra".to_owned(), PathBuf::from("projects/extra")),
        ]),
    )
    .err()
    .map(error_message)
    .unwrap_or_default();
    assert!(error.contains("has no prepared deployment artifact"));

    let error = deployment_stacks(
        ProjectId(Uuid::from_u128(1)),
        "project",
        TargetId(Uuid::from_u128(10)),
        "local",
        "development",
        vec![artifact("app"), artifact("app")],
        BTreeMap::from([("app".to_owned(), PathBuf::from("projects/app"))]),
    )
    .err()
    .map(error_message)
    .unwrap_or_default();
    assert!(error.contains("more than once"));
    Ok(())
}

#[test]
fn desired_claims_require_exact_project_ownership_and_reject_reserved_hosts() {
    let valid = claim(1, "development", "app.test", "/");
    assert!(
        validate_desired_claims(
            valid.target_id,
            valid.project_id,
            "development",
            std::slice::from_ref(&valid),
        )
        .is_ok()
    );

    let mut invalid = Vec::new();
    let mut value = valid.clone();
    value.target_id = TargetId(Uuid::from_u128(11));
    invalid.push(value);
    let mut value = valid.clone();
    value.project_id = ProjectId(Uuid::from_u128(2));
    invalid.push(value);
    let mut value = valid.clone();
    value.environment = "production".to_owned();
    invalid.push(value);
    let mut value = valid.clone();
    value.provenance = IngressRouteProvenance::MmManagedService;
    invalid.push(value);
    let mut value = valid.clone();
    value.ownership = IngressRouteOwnership::MmManaged {
        component: "dashboard".to_owned(),
    };
    invalid.push(value);
    for change in 0..4 {
        let mut value = valid.clone();
        if let IngressRouteOwnership::Project {
            project_id,
            environment,
            stack,
            service,
        } = &mut value.ownership
        {
            match change {
                0 => *project_id = ProjectId(Uuid::from_u128(2)),
                1 => *environment = "production".to_owned(),
                2 => *stack = "worker".to_owned(),
                3 => *service = "worker".to_owned(),
                _ => unreachable!(),
            }
        }
        invalid.push(value);
    }
    for value in invalid {
        let error =
            validate_desired_claims(valid.target_id, valid.project_id, "development", &[value])
                .err()
                .map(error_message)
                .unwrap_or_default();
        assert!(error.contains("consistent project ownership"));
    }

    let reserved = claim(1, "development", "dashboard.mm.test", "/");
    let error = validate_desired_claims(
        reserved.target_id,
        reserved.project_id,
        "development",
        &[reserved],
    )
    .err()
    .map(error_message)
    .unwrap_or_default();
    assert!(error.contains("reserved mm host"));
}

#[test]
fn route_conflicts_distinguish_owner_equality_from_cross_project_overlap() {
    let root = claim(1, "development", "app.test", "/api");
    let mut same_owner_nested = root.clone();
    same_owner_nested.path_prefix = "/api/v1".to_owned();
    assert!(reject_route_conflicts(&[root.clone(), same_owner_nested]).is_ok());

    let mut same_identity = root.clone();
    same_identity.route = "duplicate".to_owned();
    let error = reject_route_conflicts(&[root.clone(), same_identity])
        .err()
        .map(error_message)
        .unwrap_or_default();
    assert!(error.contains("conflicts with route"));

    let cross_owner_nested = claim(2, "development", "app.test", "/api/v1");
    assert!(reject_route_conflicts(&[root.clone(), cross_owner_nested]).is_err());
    let cross_owner_disjoint = claim(2, "development", "app.test", "/assets");
    assert!(reject_route_conflicts(&[root.clone(), cross_owner_disjoint]).is_ok());
    let other_host = claim(2, "development", "other.test", "/api");
    assert!(reject_route_conflicts(&[root, other_host]).is_ok());
}

#[test]
fn route_diff_classifies_changes_and_rejects_duplicate_identities() -> Result<(), MmError> {
    let unchanged = claim(1, "development", "same.test", "/");
    let removed = claim(1, "development", "removed.test", "/");
    let before = claim(1, "development", "changed.test", "/");
    let mut after = before.clone();
    after.port = 9090;
    let added = claim(1, "development", "added.test", "/");
    let diff = route_diff(
        &[before.clone(), removed.clone(), unchanged.clone()],
        &[after.clone(), added.clone(), unchanged],
    )?;
    assert_eq!(diff.added[0].host, added.host);
    assert_eq!(diff.changed[0].before.port, before.port);
    assert_eq!(diff.changed[0].after.port, after.port);
    assert_eq!(diff.removed[0].host, removed.host);

    assert!(route_diff(&[before.clone(), before.clone()], &[]).is_err());
    assert!(route_diff(&[], &[after.clone(), after]).is_err());
    Ok(())
}

#[test]
fn gate_attachments_are_preserved_only_for_an_unchanged_route() -> Result<(), MmError> {
    let gate_name = "mm-gate-private".to_owned();
    let ordinary_name = "rewrite".to_owned();
    let gate_definition = gate("private");
    let ordinary_definition = replace_path();
    assert!(is_gate_middleware(&gate_name, &gate_definition));
    assert!(!is_gate_middleware("private", &gate_definition));
    assert!(!is_gate_middleware(
        &gate_name,
        &CanonicalMiddleware::ForwardAuth {
            address: "https://identity.example/verify".to_owned(),
            trust_forward_header: true,
            auth_response_headers: Vec::new(),
            add_auth_cookies_to_response: Vec::new(),
        }
    ));
    assert!(!is_gate_middleware(&gate_name, &ordinary_definition));

    let mut current = claim(1, "development", "app.test", "/");
    current.middlewares = vec![
        gate_name.clone(),
        ordinary_name.clone(),
        "missing".to_owned(),
    ];
    let current_definitions = BTreeMap::from([
        (gate_name.clone(), gate_definition.clone()),
        (ordinary_name.clone(), ordinary_definition.clone()),
    ]);
    assert_eq!(
        owner_middlewares(&[current.clone()], &current_definitions),
        current_definitions
    );

    let mut desired = current.clone();
    desired.middlewares.clear();
    let mut desired_definitions = BTreeMap::new();
    preserve_gate_attachments(
        &[current.clone()],
        &current_definitions,
        std::slice::from_mut(&mut desired),
        &mut desired_definitions,
    )?;
    assert_eq!(
        desired.middlewares.as_slice(),
        std::slice::from_ref(&gate_name)
    );
    assert_eq!(desired_definitions.get(&gate_name), Some(&gate_definition));

    let stripped = claims_without_gate_attachments(&[current.clone()], &current_definitions);
    assert_eq!(
        stripped[0].middlewares,
        [ordinary_name.clone(), "missing".to_owned()]
    );
    let merged = merge_middlewares(
        &current_definitions,
        &[current.clone()],
        &BTreeMap::from([(gate_name.clone(), gate_definition.clone())]),
    );
    assert_eq!(merged.len(), 2);
    assert_eq!(merged.get(&ordinary_name), Some(&ordinary_definition));

    let error = preserve_gate_attachments(
        &[current.clone()],
        &current_definitions,
        &mut [],
        &mut BTreeMap::new(),
    )
    .err()
    .map(error_message)
    .unwrap_or_default();
    assert!(error.contains("detach it before removing"));

    let mut changed = current.clone();
    changed.host = "changed.test".to_owned();
    let error = preserve_gate_attachments(
        &[current.clone()],
        &current_definitions,
        std::slice::from_mut(&mut changed),
        &mut BTreeMap::new(),
    )
    .err()
    .map(error_message)
    .unwrap_or_default();
    assert!(error.contains("host, path, or TLS"));

    let mut conflict = BTreeMap::from([(gate_name.clone(), gate("other"))]);
    let error = preserve_gate_attachments(
        &[current.clone()],
        &current_definitions,
        std::slice::from_mut(&mut current),
        &mut conflict,
    )
    .err()
    .map(error_message)
    .unwrap_or_default();
    assert!(error.contains("conflicts with the desired"));
    Ok(())
}

#[test]
fn canonical_route_order_binds_every_identity_and_behavior_tiebreaker() {
    let base = claim(1, "a", "a.test", "/a");
    let variants = (0..12)
        .map(|field| {
            let mut right = base.clone();
            match field {
                0 => right.host = "b.test".to_owned(),
                1 => right.path_prefix = "/b".to_owned(),
                2 => right.project_id = ProjectId(Uuid::from_u128(2)),
                3 => right.environment = "b".to_owned(),
                4 => right.stack = "b".to_owned(),
                5 => right.route = "z".to_owned(),
                6 => right.service = "z".to_owned(),
                7 => right.port += 1,
                8 => right.tls = false,
                9 => right.router_name = "z".to_owned(),
                10 => right.backend_alias = "z".to_owned(),
                11 => right.artifact_hash = "z".to_owned(),
                _ => unreachable!(),
            }
            (field, right)
        })
        .collect::<Vec<_>>();
    for (field, right) in variants {
        let expected = if field == 8 {
            Ordering::Greater
        } else {
            Ordering::Less
        };
        assert_eq!(
            canonical_claim_order(&base, &right),
            expected,
            "field {field}"
        );
    }
    assert_eq!(canonical_claim_order(&base, &base), Ordering::Equal);
}
