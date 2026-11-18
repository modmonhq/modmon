use super::*;

struct IngressOwnerReplacement<'a> {
    target_id: TargetId,
    project_id: ProjectId,
    environment: &'a str,
    expected_generation: Option<u64>,
    claims: &'a [IngressRouteClaim],
    middlewares: &'a BTreeMap<String, CanonicalMiddleware>,
    preserve_gate_attachments: bool,
    config_digest: Option<&'a str>,
    now_unix_ms: i64,
}

impl SqliteStateStore {
    fn replace_ingress_owner_claims(
        &self,
        replacement: IngressOwnerReplacement<'_>,
    ) -> Result<IngressRouteSet, MmError> {
        let IngressOwnerReplacement {
            target_id,
            project_id,
            environment,
            expected_generation,
            claims,
            middlewares,
            preserve_gate_attachments,
            config_digest,
            now_unix_ms,
        } = replacement;
        validate_ingress_owner_claims(target_id, project_id, environment, claims)?;
        validate_ingress_owner_middlewares(claims, middlewares)?;
        let mut claims = claims.to_vec();
        let mut middlewares = middlewares.clone();

        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        let (generation, _) = ingress_generation(&transaction, target_id)?;
        validate_ingress_generation(generation, expected_generation, target_id)?;
        if preserve_gate_attachments {
            restore_gate_attachments(
                &transaction,
                target_id,
                project_id,
                environment,
                &mut claims,
                &mut middlewares,
            )?;
            validate_ingress_owner_middlewares(&claims, &middlewares)?;
        }
        let next_generation = increment_ingress_generation(generation, target_id)?;
        let existing = ingress_claims(&transaction, target_id)?;
        ensure_no_route_conflicts(&claims, &existing)?;
        transaction
            .execute(
                "DELETE FROM ingress_route_claims
                 WHERE target_id = ?1 AND project_id = ?2 AND environment_name = ?3",
                params![
                    target_id.0.to_string(),
                    project_id.0.to_string(),
                    environment
                ],
            )
            .map_err(state_error)?;
        for claim in &claims {
            insert_ingress_claim(&transaction, claim, now_unix_ms)?;
        }
        transaction
            .execute(
                "INSERT INTO ingress_route_generations
                    (target_id, generation, config_digest, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(target_id) DO UPDATE SET
                    generation = excluded.generation,
                    config_digest = excluded.config_digest,
                    updated_at_ms = excluded.updated_at_ms",
                params![
                    target_id.0.to_string(),
                    next_generation.to_string(),
                    config_digest,
                    now_unix_ms
                ],
            )
            .map_err(state_error)?;
        persist_owner_middlewares(
            &transaction,
            target_id,
            project_id,
            environment,
            &middlewares,
            now_unix_ms,
        )?;
        let claims = ingress_claims(&transaction, target_id)?;
        let stored_middlewares = ingress_middlewares(&transaction, target_id)?;
        transaction.commit().map_err(state_error)?;
        Ok(IngressRouteSet {
            target_id,
            generation: next_generation,
            config_digest: config_digest.map(str::to_owned),
            claims,
            middlewares: stored_middlewares,
        })
    }
}

fn validate_ingress_generation(
    generation: u64,
    expected: Option<u64>,
    target_id: TargetId,
) -> Result<(), MmError> {
    if let Some(expected) = expected
        && expected != generation
    {
        return Err(MmError::State {
            message: format!(
                "target '{}' ingress routes changed concurrently (expected generation {}, found {})",
                target_id.0, expected, generation
            ),
        });
    }
    Ok(())
}

fn increment_ingress_generation(generation: u64, target_id: TargetId) -> Result<u64, MmError> {
    generation.checked_add(1).ok_or_else(|| MmError::State {
        message: format!(
            "target '{}' exhausted its ingress route generation counter",
            target_id.0
        ),
    })
}

fn ensure_no_route_conflicts(
    claims: &[IngressRouteClaim],
    existing: &[IngressRouteClaim],
) -> Result<(), MmError> {
    for claim in claims {
        if existing.iter().any(|stored| {
            !same_route_owner(claim, stored)
                && claim.host == stored.host
                && route_paths_overlap(&claim.path_prefix, &stored.path_prefix)
        }) {
            return Err(MmError::State {
                message: format!(
                    "ingress route {}{} conflicts with an existing claim on target '{}'",
                    claim.host, claim.path_prefix, claim.target
                ),
            });
        }
    }
    Ok(())
}

fn insert_ingress_claim(
    transaction: &rusqlite::Transaction<'_>,
    claim: &IngressRouteClaim,
    now_unix_ms: i64,
) -> Result<(), MmError> {
    let middlewares =
        serde_json::to_string(&claim.middlewares).map_err(|error| MmError::State {
            message: format!("could not serialize ingress middleware names: {error}"),
        })?;
    let certificate =
        serde_json::to_string(&claim.certificate_reference).map_err(|error| MmError::State {
            message: format!("could not serialize ingress certificate reference: {error}"),
        })?;
    transaction.execute(
        "INSERT INTO ingress_route_claims (
            target_id, target_name, hostname, path_prefix, project_id, project_name,
            environment_name, stack_name, route_name, service_name,
            backend_port, tls_enabled, router_name, backend_alias, artifact_hash,
            middlewares_json, updated_at_ms, route_provenance, certificate_reference_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
        params![
            claim.target_id.0.to_string(), claim.target, claim.host, claim.path_prefix,
            claim.project_id.0.to_string(), claim.project_name, claim.environment, claim.stack,
            claim.route, claim.service, i64::from(claim.port), claim.tls, claim.router_name,
            claim.backend_alias, claim.artifact_hash, middlewares, now_unix_ms,
            ingress_provenance_sql(claim.provenance), certificate,
        ],
    ).map_err(state_error)?;
    Ok(())
}

impl IngressRouteRegistry for SqliteStateStore {
    fn route_set(&self, target_id: TargetId) -> Result<IngressRouteSet, MmError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(state_error)?;
        let (generation, config_digest) = ingress_generation(&transaction, target_id)?;
        let claims = ingress_claims(&transaction, target_id)?;
        let middlewares = ingress_middlewares(&transaction, target_id)?;
        transaction.commit().map_err(state_error)?;
        Ok(IngressRouteSet {
            target_id,
            generation,
            config_digest,
            claims,
            middlewares,
        })
    }

    fn conflicts(
        &self,
        proposed: &[IngressRouteClaim],
    ) -> Result<Vec<IngressRouteConflict>, MmError> {
        let connection = self.connection()?;
        let mut conflicts = Vec::new();
        for claim in proposed {
            for existing in ingress_claims(&connection, claim.target_id)? {
                if same_route_owner(claim, &existing) {
                    continue;
                }
                if claim.host == existing.host
                    && route_paths_overlap(&claim.path_prefix, &existing.path_prefix)
                {
                    conflicts.push(IngressRouteConflict {
                        proposed: claim.clone(),
                        existing,
                    });
                }
            }
        }
        Ok(conflicts)
    }

    fn replace_owner_claims_cas(
        &self,
        replacement: crate::app::ports::IngressClaimsReplacement<'_>,
    ) -> Result<IngressRouteSet, MmError> {
        self.replace_ingress_owner_claims(IngressOwnerReplacement {
            target_id: replacement.target_id,
            project_id: replacement.project_id,
            environment: replacement.environment,
            expected_generation: Some(replacement.expected_generation),
            claims: replacement.claims,
            middlewares: replacement.middlewares,
            preserve_gate_attachments: replacement.preserve_gate_attachments,
            config_digest: replacement.config_digest,
            now_unix_ms: replacement.now_unix_ms,
        })
    }

    fn replace_owner_claims(
        &self,
        target_id: TargetId,
        project_id: ProjectId,
        environment: &str,
        claims: &[IngressRouteClaim],
        middlewares: &BTreeMap<String, CanonicalMiddleware>,
        now_unix_ms: i64,
    ) -> Result<(), MmError> {
        self.replace_ingress_owner_claims(IngressOwnerReplacement {
            target_id,
            project_id,
            environment,
            expected_generation: None,
            claims,
            middlewares,
            preserve_gate_attachments: true,
            config_digest: None,
            now_unix_ms,
        })?;
        Ok(())
    }
}

fn restore_gate_attachments(
    connection: &rusqlite::Transaction<'_>,
    target_id: TargetId,
    project_id: ProjectId,
    environment: &str,
    claims: &mut [IngressRouteClaim],
    middlewares: &mut BTreeMap<String, CanonicalMiddleware>,
) -> Result<(), MmError> {
    let existing_middlewares = ingress_middlewares(connection, target_id)?;
    let mut statement = connection
        .prepare(
            "SELECT router_name,host,path_prefix,middleware_name
             FROM gate_route_attachments
             WHERE target_id=?1 AND project_id=?2 AND environment_name=?3
             ORDER BY router_name",
        )
        .map_err(state_error)?;
    let attachments = statement
        .query_map(
            params![
                target_id.0.to_string(),
                project_id.0.to_string(),
                environment
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .map_err(state_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(state_error)?;
    drop(statement);

    for (router_name, host, path_prefix, middleware_name) in attachments {
        let claim = claims
            .iter_mut()
            .find(|claim| claim.router_name == router_name)
            .ok_or_else(|| MmError::State {
                message: format!(
                    "ingress router '{router_name}' is access-gated; detach it before removing or renaming the route"
                ),
            })?;
        if claim.host != host || claim.path_prefix != path_prefix || !claim.tls {
            return Err(MmError::State {
                message: format!(
                    "ingress router '{router_name}' is access-gated; detach it before changing its host, path, or TLS mode"
                ),
            });
        }
        let definition = existing_middlewares.get(&middleware_name).ok_or_else(|| {
            MmError::Security {
                message: format!(
                    "access-gate middleware '{middleware_name}' is missing from owned ingress state"
                ),
            }
        })?;
        if middlewares
            .get(&middleware_name)
            .is_some_and(|incoming| incoming != definition)
        {
            return Err(MmError::Security {
                message: format!(
                    "access-gate middleware '{middleware_name}' conflicts with compiled ingress state"
                ),
            });
        }
        middlewares.insert(middleware_name.clone(), definition.clone());
        claim.middlewares.push(middleware_name);
        claim.middlewares.sort();
        claim.middlewares.dedup();
    }
    Ok(())
}

fn validate_ingress_owner_claims(
    target_id: TargetId,
    project_id: ProjectId,
    environment: &str,
    claims: &[IngressRouteClaim],
) -> Result<(), MmError> {
    if claims.iter().any(|claim| {
        claim.target_id != target_id
            || claim.project_id != project_id
            || claim.environment != environment
            || claim.provenance == IngressRouteProvenance::MmManagedService
            || !matches!(
                &claim.ownership,
                IngressRouteOwnership::Project {
                    project_id: owner_project_id,
                    environment: owner_environment,
                    stack,
                    service,
                } if *owner_project_id == claim.project_id
                    && owner_environment == &claim.environment
                    && stack == &claim.stack
                    && service == &claim.service
            )
    }) {
        return Err(MmError::State {
            message: "ingress claims do not have consistent project ownership".to_owned(),
        });
    }
    if let Some(claim) = claims
        .iter()
        .find(|claim| crate::domain::local_ingress::is_reserved_mm_host(&claim.host))
    {
        return Err(MmError::State {
            message: format!(
                "project ingress claim cannot use reserved mm host '{}'",
                claim.host
            ),
        });
    }
    for (index, claim) in claims.iter().enumerate() {
        if claims
            .iter()
            .skip(index + 1)
            .any(|other| claim.host == other.host && claim.path_prefix == other.path_prefix)
        {
            return Err(MmError::State {
                message: "one route owner cannot publish duplicate ingress claims".to_owned(),
            });
        }
    }
    Ok(())
}

fn validate_ingress_owner_middlewares(
    claims: &[IngressRouteClaim],
    middlewares: &BTreeMap<String, CanonicalMiddleware>,
) -> Result<(), MmError> {
    for claim in claims {
        for name in &claim.middlewares {
            if !middlewares.contains_key(name) {
                return Err(MmError::State {
                    message: format!(
                        "ingress claim '{}' references unknown middleware '{name}'",
                        claim.router_name
                    ),
                });
            }
        }
    }
    Ok(())
}

fn persist_owner_middlewares(
    connection: &rusqlite::Transaction<'_>,
    target_id: TargetId,
    project_id: ProjectId,
    environment: &str,
    middlewares: &BTreeMap<String, CanonicalMiddleware>,
    now_unix_ms: i64,
) -> Result<(), MmError> {
    let existing = ingress_middlewares(connection, target_id)?;
    for (name, definition) in middlewares {
        if let Some(existing) = existing.get(name) {
            let owner = middleware_owner(connection, target_id, name)?;
            if owner.as_ref().is_some_and(|(owner_project, owner_env)| {
                *owner_project != project_id || owner_env != environment
            }) && existing != definition
            {
                return Err(MmError::State {
                    message: format!("middleware '{name}' has conflicting definitions"),
                });
            }
        }
    }
    connection
        .execute(
            "DELETE FROM ingress_middleware_definitions
             WHERE target_id = ?1 AND project_id = ?2 AND environment_name = ?3",
            params![
                target_id.0.to_string(),
                project_id.0.to_string(),
                environment
            ],
        )
        .map_err(state_error)?;
    for (name, definition) in middlewares {
        let definition_json =
            serde_json::to_string(definition).map_err(|error| MmError::State {
                message: format!("could not serialize ingress middleware '{name}': {error}"),
            })?;
        let definition_digest = definition
            .digest()
            .map_err(|message| MmError::State { message })?;
        connection
            .execute(
                "INSERT INTO ingress_middleware_definitions (
                    target_id, project_id, environment_name, namespaced_name,
                    definition_json, definition_digest, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    target_id.0.to_string(),
                    project_id.0.to_string(),
                    environment,
                    name,
                    definition_json,
                    definition_digest,
                    now_unix_ms
                ],
            )
            .map_err(state_error)?;
    }
    Ok(())
}

fn middleware_owner(
    connection: &Connection,
    target_id: TargetId,
    name: &str,
) -> Result<Option<(ProjectId, String)>, MmError> {
    connection
        .query_row(
            "SELECT project_id, environment_name
             FROM ingress_middleware_definitions
             WHERE target_id = ?1 AND namespaced_name = ?2",
            params![target_id.0.to_string(), name],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(state_error)?
        .map(|(project_id, environment)| {
            Ok((
                ProjectId(
                    Uuid::parse_str(&project_id).map_err(|error| MmError::State {
                        message: format!("stored middleware project ID is invalid: {error}"),
                    })?,
                ),
                environment,
            ))
        })
        .transpose()
}

fn ingress_middlewares(
    connection: &Connection,
    target_id: TargetId,
) -> Result<BTreeMap<String, CanonicalMiddleware>, MmError> {
    let mut statement = connection
        .prepare(
            "SELECT namespaced_name, definition_json
             FROM ingress_middleware_definitions
             WHERE target_id = ?1
             ORDER BY namespaced_name",
        )
        .map_err(state_error)?;
    let rows = statement
        .query_map(params![target_id.0.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(state_error)?;
    let mut middlewares = BTreeMap::new();
    for row in rows {
        let (name, json) = row.map_err(state_error)?;
        let definition =
            serde_json::from_str::<CanonicalMiddleware>(&json).map_err(|error| MmError::State {
                message: format!("stored middleware '{name}' is invalid: {error}"),
            })?;
        middlewares.insert(name, definition);
    }
    Ok(middlewares)
}

fn ingress_generation(
    connection: &Connection,
    target_id: TargetId,
) -> Result<(u64, Option<String>), MmError> {
    let stored = connection
        .query_row(
            "SELECT generation, config_digest
             FROM ingress_route_generations
             WHERE target_id = ?1",
            params![target_id.0.to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()
        .map_err(state_error)?;
    let Some((generation, config_digest)) = stored else {
        return Ok((0, None));
    };
    let generation = generation.parse::<u64>().map_err(|error| MmError::State {
        message: format!(
            "stored ingress route generation for target '{}' is invalid: {error}",
            target_id.0
        ),
    })?;
    Ok((generation, config_digest))
}

fn ingress_claims(
    connection: &Connection,
    target_id: TargetId,
) -> Result<Vec<IngressRouteClaim>, MmError> {
    let mut statement = connection
        .prepare(
            "SELECT target_name, project_id, project_name, environment_name, stack_name, route_name,
                    hostname, path_prefix, service_name, backend_port, tls_enabled,
                    router_name, backend_alias, artifact_hash, middlewares_json,
                    route_provenance, certificate_reference_json
             FROM ingress_route_claims
             WHERE target_id = ?1
             ORDER BY hostname, path_prefix, router_name, backend_alias, backend_port, tls_enabled",
        )
        .map_err(state_error)?;
    let rows = statement
        .query_map(params![target_id.0.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, bool>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, String>(13)?,
                row.get::<_, String>(14)?,
                row.get::<_, String>(15)?,
                row.get::<_, String>(16)?,
            ))
        })
        .map_err(state_error)?;
    rows.map(|row| {
        let (
            target,
            project_id,
            project_name,
            environment,
            stack,
            route,
            host,
            path_prefix,
            service,
            port,
            tls,
            router_name,
            backend_alias,
            artifact_hash,
            middlewares_json,
            provenance,
            certificate_reference_json,
        ) = row.map_err(state_error)?;
        let project_id = stored_ingress_project_id(&project_id)?;
        let certificate_reference =
            serde_json::from_str(&certificate_reference_json).map_err(|error| MmError::State {
                message: format!("stored ingress certificate reference is invalid: {error}"),
            })?;
        let ownership = IngressRouteOwnership::Project {
            project_id,
            environment: environment.clone(),
            stack: stack.clone(),
            service: service.clone(),
        };
        Ok(IngressRouteClaim {
            target,
            target_id,
            project_id,
            project_name,
            environment,
            stack,
            route,
            host,
            path_prefix: path_prefix.clone(),
            service,
            port: u16::try_from(port).map_err(|_| MmError::State {
                message: "stored ingress backend port is invalid".to_owned(),
            })?,
            tls,
            provenance: parse_ingress_provenance(&provenance)?,
            certificate_reference,
            ownership,
            priority: route_priority(&path_prefix),
            router_name,
            backend_alias,
            artifact_hash,
            middlewares: serde_json::from_str(&middlewares_json).map_err(|error| {
                MmError::State {
                    message: format!("stored ingress middleware names are invalid: {error}"),
                }
            })?,
        })
    })
    .collect()
}

fn stored_ingress_project_id(value: &str) -> Result<ProjectId, MmError> {
    Uuid::parse_str(value)
        .map(ProjectId)
        .map_err(|error| MmError::State {
            message: format!("stored ingress project ID is invalid: {error}"),
        })
}

const fn ingress_provenance_sql(provenance: IngressRouteProvenance) -> &'static str {
    match provenance {
        IngressRouteProvenance::ExplicitConfig => "explicit_config",
        IngressRouteProvenance::AutomaticConvention => "automatic_convention",
        IngressRouteProvenance::MmManagedService => "mm_managed_service",
    }
}

fn parse_ingress_provenance(value: &str) -> Result<IngressRouteProvenance, MmError> {
    match value {
        "explicit_config" => Ok(IngressRouteProvenance::ExplicitConfig),
        "automatic_convention" => Ok(IngressRouteProvenance::AutomaticConvention),
        "mm_managed_service" => Ok(IngressRouteProvenance::MmManagedService),
        _ => Err(MmError::State {
            message: format!("stored ingress route provenance '{value}' is invalid"),
        }),
    }
}

fn same_route_owner(left: &IngressRouteClaim, right: &IngressRouteClaim) -> bool {
    left.target_id == right.target_id
        && left.project_id == right.project_id
        && left.environment == right.environment
}
