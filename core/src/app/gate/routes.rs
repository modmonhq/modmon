use super::*;

impl GateManager {
    pub fn plan_attach(
        &self,
        project_id: ProjectId,
        environment: &str,
        policy_name: &str,
        target_id: TargetId,
        router_name: &str,
        attach: bool,
    ) -> Result<GateMutationPlan, MmError> {
        let policy = self.policy(project_id, environment, policy_name)?;
        let routes = self.dependencies.routes.route_set(target_id)?;
        let claim = routes
            .claims
            .iter()
            .find(|claim| claim.router_name == router_name)
            .ok_or_else(|| MmError::State {
                message: "owned ingress route not found".to_owned(),
            })?;
        if claim.project_id != project_id || claim.environment != environment {
            return Err(MmError::Security {
                message: "the route is not owned by the selected project and environment"
                    .to_owned(),
            });
        }
        if !claim.tls {
            return Err(MmError::Security {
                message: "access gates may only attach to trusted HTTPS routes".to_owned(),
            });
        }
        let existing = self
            .dependencies
            .repository
            .attachment(target_id, router_name)?;
        if attach && existing.is_some() {
            return Err(MmError::State {
                message: "the route already has an mm gate attachment".to_owned(),
            });
        }
        if !attach
            && existing
                .as_ref()
                .is_none_or(|value| value.policy_id != policy.id)
        {
            return Err(MmError::State {
                message: "the route is not attached to that gate policy".to_owned(),
            });
        }
        plan(GateMutationPlanInput {
            action: if attach {
                GateMutationAction::AttachRoute
            } else {
                GateMutationAction::DetachRoute
            },
            policy: policy_name,
            project_id,
            environment,
            subject: router_name.to_owned(),
            policy_revision: Some(policy.revision),
            subject_revision: None,
            ingress_generation: Some(routes.generation),
            requires_hidden_input: false,
            effects: vec![format!(
                "{} one namespaced forward-auth middleware on {}{}",
                if attach { "attach" } else { "detach" },
                claim.host,
                claim.path_prefix
            )],
        })
    }

    pub async fn apply_attach_reviewed(
        &self,
        request: ApplyGateRouteRequest<'_>,
    ) -> Result<GateRouteAttachment, MmError> {
        let _target_lock = self.dependencies.target_locks.acquire(request.target_id)?;
        let preview = self.plan_attach(
            request.project_id,
            request.environment,
            request.policy_name,
            request.target_id,
            request.router_name,
            request.attach,
        )?;
        require_review(&preview, request.reviewed_plan_hash)?;
        let policy = self.policy(request.project_id, request.environment, request.policy_name)?;
        let mut routes = self.dependencies.routes.route_set(request.target_id)?;
        if preview.policy_revision != Some(policy.revision)
            || preview.ingress_generation != Some(routes.generation)
        {
            return Err(stale());
        }
        let transaction = prepare_route_transaction(
            RouteTransactionInput {
                policy: &policy,
                project_id: request.project_id,
                environment: request.environment,
                target_id: request.target_id,
                router_name: request.router_name,
                attach: request.attach,
                now_unix_ms: self.dependencies.clock.now_unix_ms(),
            },
            &mut routes,
        )?;
        self.publish_attached_route(&transaction, &routes).await?;
        let applied_routes = self.replace_route_claims(&transaction, &routes).await?;
        self.commit_route_metadata(&transaction, &applied_routes)
            .await?;
        self.publish_detached_route(&transaction, &routes, &applied_routes)
            .await?;
        self.audit(&preview)?;
        Ok(transaction.attachment)
    }

    async fn publish_attached_route(
        &self,
        transaction: &RouteTransaction,
        routes: &IngressRouteSet,
    ) -> Result<(), MmError> {
        if !transaction.attach {
            return Ok(());
        }
        let published = self
            .dependencies
            .ingress
            .publish(transaction.target_id, &routes.claims, &routes.middlewares)
            .await;
        let published_digest = match published {
            Ok(digest) => digest,
            Err(error) => {
                return match self
                    .dependencies
                    .ingress
                    .publish(
                        transaction.target_id,
                        &transaction.previous_claims,
                        &transaction.previous_middlewares,
                    )
                    .await
                {
                    Ok(_) => Err(error),
                    Err(rollback) => Err(MmError::operation_failure(
                        transaction.desired_digest.clone(),
                        crate::domain::error::MutationStatus::Unknown,
                        error,
                        vec![rollback.to_string()],
                        "restore the previous ingress claims and dynamic configuration before retrying",
                    )),
                };
            }
        };
        if published_digest != transaction.desired_digest {
            let primary = MmError::Security {
                message: "published access-gate ingress digest does not match the reviewed routes"
                    .to_owned(),
            };
            let rollback = self
                .dependencies
                .ingress
                .publish(
                    transaction.target_id,
                    &transaction.previous_claims,
                    &transaction.previous_middlewares,
                )
                .await;
            return Err(match rollback {
                Ok(_) => primary,
                Err(cleanup) => MmError::operation_failure(
                    transaction.desired_digest.clone(),
                    crate::domain::error::MutationStatus::Unknown,
                    primary,
                    vec![cleanup.to_string()],
                    "restore the previous ingress claims and dynamic configuration before retrying",
                ),
            });
        }
        Ok(())
    }

    async fn replace_route_claims(
        &self,
        transaction: &RouteTransaction,
        routes: &IngressRouteSet,
    ) -> Result<IngressRouteSet, MmError> {
        match self
            .dependencies
            .routes
            .replace_owner_claims_cas(IngressClaimsReplacement {
                target_id: transaction.target_id,
                project_id: transaction.project_id,
                environment: &transaction.environment,
                expected_generation: routes.generation,
                claims: &routes.claims,
                middlewares: &routes.middlewares,
                preserve_gate_attachments: false,
                config_digest: Some(&transaction.desired_digest),
                now_unix_ms: self.dependencies.clock.now_unix_ms(),
            }) {
            Ok(routes) => Ok(routes),
            Err(error) => {
                if transaction.attach
                    && let Err(rollback) = self
                        .dependencies
                        .ingress
                        .publish(
                            transaction.target_id,
                            &transaction.previous_claims,
                            &transaction.previous_middlewares,
                        )
                        .await
                {
                    return Err(MmError::State {
                        message: format!(
                            "gate attachment failed and ingress rollback failed: {error}; {rollback}"
                        ),
                    });
                }
                Err(error)
            }
        }
    }

    async fn commit_route_metadata(
        &self,
        transaction: &RouteTransaction,
        applied_routes: &IngressRouteSet,
    ) -> Result<(), MmError> {
        let result = if transaction.attach {
            self.dependencies
                .repository
                .insert_attachment(&transaction.attachment)
                .map(|()| true)
        } else {
            self.dependencies.repository.remove_attachment(
                transaction.target_id,
                &transaction.attachment.router_name,
                transaction.attachment.policy_id,
            )
        };
        let result = result.and_then(|changed| changed.then_some(()).ok_or_else(stale));
        if let Err(error) = result {
            let rollback =
                self.dependencies
                    .routes
                    .replace_owner_claims_cas(IngressClaimsReplacement {
                        target_id: transaction.target_id,
                        project_id: transaction.project_id,
                        environment: &transaction.environment,
                        expected_generation: applied_routes.generation,
                        claims: &transaction.previous_claims,
                        middlewares: &transaction.previous_middlewares,
                        preserve_gate_attachments: false,
                        config_digest: transaction.previous_config_digest.as_deref(),
                        now_unix_ms: self.dependencies.clock.now_unix_ms(),
                    });
            return match rollback {
                Ok(_) => match self
                    .dependencies
                    .ingress
                    .publish(
                        transaction.target_id,
                        &transaction.previous_claims,
                        &transaction.previous_middlewares,
                    )
                    .await
                {
                    Ok(_) => Err(error),
                    Err(publish) => Err(MmError::State {
                        message: format!(
                            "gate attachment failed and ingress rollback failed: {error}; {publish}"
                        ),
                    }),
                },
                Err(rollback) => Err(MmError::State {
                    message: format!(
                        "gate attachment failed and ingress rollback failed: {error}; {rollback}"
                    ),
                }),
            };
        }
        Ok(())
    }

    async fn publish_detached_route(
        &self,
        transaction: &RouteTransaction,
        routes: &IngressRouteSet,
        applied_routes: &IngressRouteSet,
    ) -> Result<(), MmError> {
        if transaction.attach {
            return Ok(());
        }
        let publish = self
            .dependencies
            .ingress
            .publish(transaction.target_id, &routes.claims, &routes.middlewares)
            .await;
        let publish = match publish {
            Ok(digest) if digest == transaction.desired_digest => Ok(()),
            Ok(_) => Err(MmError::Security {
                message: "published access-gate ingress digest does not match the reviewed routes"
                    .to_owned(),
            }),
            Err(error) => Err(error),
        };
        if let Err(error) = publish {
            let metadata = self
                .dependencies
                .repository
                .insert_attachment(&transaction.attachment);
            let route =
                self.dependencies
                    .routes
                    .replace_owner_claims_cas(IngressClaimsReplacement {
                        target_id: transaction.target_id,
                        project_id: transaction.project_id,
                        environment: &transaction.environment,
                        expected_generation: applied_routes.generation,
                        claims: &transaction.previous_claims,
                        middlewares: &transaction.previous_middlewares,
                        preserve_gate_attachments: false,
                        config_digest: transaction.previous_config_digest.as_deref(),
                        now_unix_ms: self.dependencies.clock.now_unix_ms(),
                    });
            let republish = self
                .dependencies
                .ingress
                .publish(
                    transaction.target_id,
                    &transaction.previous_claims,
                    &transaction.previous_middlewares,
                )
                .await;
            if metadata.is_err() || route.is_err() || republish.is_err() {
                return Err(MmError::State {
                    message: format!("gate detachment failed and rollback was incomplete: {error}"),
                });
            }
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn policy(
        &self,
        project_id: ProjectId,
        environment: &str,
        name: &str,
    ) -> Result<GatePolicy, MmError> {
        self.dependencies
            .repository
            .policy(project_id, environment, name)?
            .ok_or_else(|| MmError::State {
                message: format!("gate policy '{name}' not found"),
            })
    }
    pub(super) async fn resolve_secret(
        &self,
        policy: &GatePolicy,
        reference: &SecretReference,
    ) -> Result<SecretMaterial, MmError> {
        self.dependencies
            .secret_resolver
            .resolve(&SecretScope::new(&policy.environment), reference)
            .await?
            .material
            .ok_or_else(|| MmError::Secret {
                message: "required gate secret is unavailable".to_owned(),
            })
    }

    pub(super) fn audit(&self, plan: &GateMutationPlan) -> Result<(), MmError> {
        self.dependencies.repository.append_audit(&GateAuditRecord {
            id: Uuid::new_v4(),
            action: plan.action,
            project_id: plan.project_id,
            environment: plan.environment.clone(),
            policy: plan.policy.clone(),
            subject: plan.subject.clone(),
            plan_hash: plan.plan_hash.clone(),
            occurred_at_unix_ms: self.dependencies.clock.now_unix_ms(),
        })
    }
}

struct RouteTransaction {
    project_id: ProjectId,
    environment: String,
    target_id: TargetId,
    attach: bool,
    previous_claims: Vec<IngressRouteClaim>,
    previous_middlewares: BTreeMap<String, CanonicalMiddleware>,
    previous_config_digest: Option<String>,
    desired_digest: String,
    attachment: GateRouteAttachment,
}

struct RouteTransactionInput<'a> {
    policy: &'a GatePolicy,
    project_id: ProjectId,
    environment: &'a str,
    target_id: TargetId,
    router_name: &'a str,
    attach: bool,
    now_unix_ms: i64,
}

fn prepare_route_transaction(
    input: RouteTransactionInput<'_>,
    routes: &mut IngressRouteSet,
) -> Result<RouteTransaction, MmError> {
    let previous_claims = routes.claims.clone();
    let previous_middlewares = routes.middlewares.clone();
    let previous_config_digest = routes.config_digest.clone();
    let claim = routes
        .claims
        .iter_mut()
        .find(|claim| claim.router_name == input.router_name)
        .ok_or_else(stale)?;
    let middleware = input.policy.forward_auth_middleware_name(input.target_id);
    let attachment = GateRouteAttachment {
        policy_id: input.policy.id,
        target_id: input.target_id,
        project_id: input.project_id,
        environment: input.environment.to_owned(),
        router_name: input.router_name.to_owned(),
        host: claim.host.clone(),
        path_prefix: claim.path_prefix.clone(),
        middleware_name: middleware.clone(),
        attached_at_unix_ms: input.now_unix_ms,
    };
    if input.attach {
        claim.middlewares.push(middleware.clone());
        claim.middlewares.sort();
        claim.middlewares.dedup();
        routes.middlewares.insert(
            middleware,
            CanonicalMiddleware::ForwardAuth {
                address: format!(
                    "http://mm-control-host:42831/_mm/gate/forward/{}",
                    input.policy.id
                ),
                trust_forward_header: false,
                auth_response_headers: vec!["X-MM-Gate-Principal".to_owned()],
                add_auth_cookies_to_response: vec![gate_session_cookie_name(input.policy.id)],
            },
        );
    } else {
        claim.middlewares.retain(|name| name != &middleware);
        routes.middlewares.remove(&middleware);
    }
    Ok(RouteTransaction {
        project_id: input.project_id,
        environment: input.environment.to_owned(),
        target_id: input.target_id,
        attach: input.attach,
        previous_claims,
        previous_middlewares,
        previous_config_digest,
        desired_digest: route_config_digest(input.target_id, &routes.claims, &routes.middlewares)?,
        attachment,
    })
}
