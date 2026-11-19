PRAGMA foreign_keys=OFF;
BEGIN TRANSACTION;
CREATE TABLE schema_migrations (
                version TEXT PRIMARY KEY NOT NULL,
                applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
INSERT INTO "schema_migrations" VALUES('0001_initial','2026-11-19 18:22:57');
CREATE TABLE workspaces (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    root TEXT NOT NULL UNIQUE,
    config_path TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    last_seen_at_ms INTEGER NOT NULL
);
INSERT INTO "workspaces" VALUES('38fa4aab-5223-4d5d-b886-6efee1b361a6','00010000-0000-4000-8000-000000000001','/work/web','/work/web/.config/mm.yaml',1000,1000);
CREATE TABLE operations (
    id TEXT PRIMARY KEY NOT NULL,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    kind TEXT NOT NULL,
    environment TEXT NOT NULL,
    state TEXT NOT NULL,
    requested_at_ms INTEGER NOT NULL,
    finished_at_ms INTEGER,
    plan_hash TEXT,
    error_code TEXT
);
INSERT INTO "operations" VALUES('00010000-0000-4000-8000-000000000003','38fa4aab-5223-4d5d-b886-6efee1b361a6','build','production','succeeded',1500,2000,'9e85a5f2155443594284b9ecf2854dfa951502308983118c731c7491d831c1a8',NULL);
INSERT INTO "operations" VALUES('00010000-0000-4000-8000-000000000004','38fa4aab-5223-4d5d-b886-6efee1b361a6','deploy','production','succeeded',1500,3200,'ff2fc70899a737473e78e5eb76125c74112a0a79cbdad55a85833c20a9e02b5f',NULL);
CREATE TABLE operation_events (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    operation_id TEXT NOT NULL REFERENCES operations(id) ON DELETE CASCADE,
    occurred_at_ms INTEGER NOT NULL,
    payload_json TEXT NOT NULL
);
INSERT INTO "operation_events" VALUES(1,'00010000-0000-4000-8000-000000000003',1500,'{"type":"started","operation_id":"00010000-0000-4000-8000-000000000003","kind":"build"}');
INSERT INTO "operation_events" VALUES(2,'00010000-0000-4000-8000-000000000003',2000,'{"type":"completed","operation_id":"00010000-0000-4000-8000-000000000003","state":"succeeded"}');
INSERT INTO "operation_events" VALUES(3,'00010000-0000-4000-8000-000000000004',1500,'{"type":"started","operation_id":"00010000-0000-4000-8000-000000000004","kind":"deploy"}');
INSERT INTO "operation_events" VALUES(4,'00010000-0000-4000-8000-000000000004',3100,'{"type":"phase_changed","operation_id":"00010000-0000-4000-8000-000000000004","phase":"render"}');
INSERT INTO "operation_events" VALUES(5,'00010000-0000-4000-8000-000000000004',3200,'{"type":"completed","operation_id":"00010000-0000-4000-8000-000000000004","state":"succeeded"}');
CREATE TABLE deployment_snapshots (
    operation_id TEXT PRIMARY KEY NOT NULL REFERENCES operations(id) ON DELETE CASCADE,
    desired_json TEXT NOT NULL,
    observed_before_json TEXT NOT NULL,
    observed_after_json TEXT,
    config_hash TEXT NOT NULL,
    rendered_hash TEXT NOT NULL
);
INSERT INTO "deployment_snapshots" VALUES('00010000-0000-4000-8000-000000000004','{"schema":"mm-local-deployment-plan-v1","project_id":"00010000-0000-4000-8000-000000000001","project_name":"web","target_name":"local","target_id":"00010000-0000-4000-8000-000000000002","target_fingerprint":"a738f54c90c9c1bb3a634fc8f83f6858adcdd6955c311ad5fe2de8fe9310d1bc","environment":"production","runtime":"docker-compose","runtime_source":null,"workload_intent_hash":null,"networks":[],"expected_route_generation":1,"expected_route_config_digest":null,"expected_previous_plan_hash":null,"stacks":[],"removed_stacks":[],"preserved_volumes":[],"removed_preserved_volumes":[],"previous_owner_claims":[],"desired_route_claims":[],"complete_route_claims":[],"desired_middlewares":{},"environment_review":{"input_hash":"","services":[],"delivery":{"mode":"direct","provenance":"built-in-default"}},"runtime_secret_plan":null,"scheduler_reconciliation":null,"environment_bundle_root":null,"previous_dynamic_config":{"root":"/srv/web/.mm/ingress/dynamic","path":"/srv/web/.mm/ingress/dynamic/routes.yaml","manifest":{"target_id":"00010000-0000-4000-8000-000000000002","claim_count":0,"router_count":0,"service_count":0,"digest":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"}},"dynamic_config":{"root":"/srv/web/.mm/ingress/dynamic","path":"/srv/web/.mm/ingress/dynamic/routes.yaml","manifest":{"target_id":"00010000-0000-4000-8000-000000000002","claim_count":0,"router_count":0,"service_count":0,"digest":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"}},"local_certificates":[],"static_ingress":null,"removed_ingress":null,"route_diff":{"added":[],"changed":[],"removed":[]},"ingress":[],"route_state_changed":false,"rollout":{"convergence_timeout_seconds":0,"stack_order":[]},"release":{"commit":"3f9a1c2e7b4d5a6f8091a2b3c4d5e6f708192a3b","name":"1.4.0"},"plan_hash":"ff2fc70899a737473e78e5eb76125c74112a0a79cbdad55a85833c20a9e02b5f"}','{}',NULL,'ff2fc70899a737473e78e5eb76125c74112a0a79cbdad55a85833c20a9e02b5f','cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc');
CREATE TABLE image_build_snapshots (
    operation_id TEXT PRIMARY KEY NOT NULL REFERENCES operations(id) ON DELETE CASCADE,
    plan_json TEXT NOT NULL,
    result_json TEXT
);
INSERT INTO "image_build_snapshots" VALUES('00010000-0000-4000-8000-000000000003','{"schema":"mm-image-build-plan-v1","project_id":"00010000-0000-4000-8000-000000000001","config_hash":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","environment":"production","source":{"commit":"3f9a1c2e7b4d5a6f8091a2b3c4d5e6f708192a3b","branch":"main","tags_at_head":[],"repository_tags":[],"tags_complete":true,"default_branch":true,"dirty":false},"publish":true,"tag_strategy":"auto","platforms":["linux/amd64"],"units":[{"module":"web","image":"registry.example.test/web","context":"web","dockerfile":"web/Dockerfile","context_hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","dockerfile_hash":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","args":{"VERSION":"3f9a1c2e"},"secret_ids":[],"dependencies":[],"upstream_node_ids":[],"extra_inputs":[],"version_embedded":false,"reuse_policy":"allowed","input_fingerprint":"59ac0208f1c6c2a9abca5606e550464576fe6c9276bb4680516027fb03d506ae","variants":[{"target":null,"tag_suffix":"","tags":["edge","sha-3f9a1c2e7b4d5a6f8091a2b3c4d5e6f708192a3b"]}]}],"plan_hash":"9e85a5f2155443594284b9ecf2854dfa951502308983118c731c7491d831c1a8"}','{"operation_id":"00010000-0000-4000-8000-000000000003","plan_hash":"9e85a5f2155443594284b9ecf2854dfa951502308983118c731c7491d831c1a8","images":[{"module":"web","target":null,"repository":"registry.example.test/web","tags":["registry.example.test/web:edge","registry.example.test/web:sha-3f9a1c2e7b4d5a6f8091a2b3c4d5e6f708192a3b"],"digest":"sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd","execution_fingerprint":"2c52bb4813d3a0f3e68f10cc2900948558c22257d4f079b676ed766c940aa479","archive":null}]}');
CREATE TABLE image_build_unit_cache (
    project_id TEXT NOT NULL,
    environment TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    module TEXT NOT NULL,
    variant_target TEXT NOT NULL,
    repository TEXT NOT NULL,
    digest TEXT NOT NULL,
    operation_id TEXT NOT NULL REFERENCES operations(id) ON DELETE CASCADE,
    PRIMARY KEY (project_id, environment, fingerprint, module, variant_target)
);
CREATE TABLE target_bootstrap_snapshots (
    operation_id TEXT PRIMARY KEY NOT NULL REFERENCES operations(id) ON DELETE CASCADE,
    plan_json TEXT NOT NULL,
    result_json TEXT
);
CREATE TABLE resource_locks (
    resource TEXT PRIMARY KEY NOT NULL,
    owner_operation_id TEXT NOT NULL,
    acquired_at_ms INTEGER NOT NULL,
    expires_at_ms INTEGER NOT NULL
);
CREATE TABLE ingress_route_claims (
    target_id TEXT NOT NULL,
    target_name TEXT NOT NULL,
    hostname TEXT NOT NULL,
    path_prefix TEXT NOT NULL,
    project_id TEXT NOT NULL,
    project_name TEXT NOT NULL,
    environment_name TEXT NOT NULL,
    stack_name TEXT NOT NULL,
    route_name TEXT NOT NULL,
    service_name TEXT NOT NULL,
    backend_port INTEGER NOT NULL CHECK (backend_port > 0 AND backend_port <= 65535),
    tls_enabled INTEGER NOT NULL CHECK (tls_enabled IN (0, 1)),
    router_name TEXT NOT NULL,
    backend_alias TEXT NOT NULL,
    artifact_hash TEXT NOT NULL,
    middlewares_json TEXT NOT NULL,
    route_provenance TEXT NOT NULL,
    certificate_reference_json TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (target_id, hostname, path_prefix)
);
INSERT INTO "ingress_route_claims" VALUES('00010000-0000-4000-8000-000000000002','local','web.example.test','/','00010000-0000-4000-8000-000000000001','web','production','app','web','web',8080,1,'mm-web','mm-ingress-backend-web','eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee','["mm-strip-api"]','explicit_config','{"kind":"local_ca","host":"web.example.test"}',2100);
CREATE TABLE ingress_middleware_definitions (
    target_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    environment_name TEXT NOT NULL,
    namespaced_name TEXT NOT NULL,
    definition_json TEXT NOT NULL,
    definition_digest TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (target_id, namespaced_name)
);
INSERT INTO "ingress_middleware_definitions" VALUES('00010000-0000-4000-8000-000000000002','00010000-0000-4000-8000-000000000001','production','mm-strip-api','{"replacePathRegex":{"regex":"^/api(.*)","replacement":"$1"}}','1998b900d7bfc740d7355b8710ea7af15afa91ebd4251ff032ab55f8aa764a07',2100);
CREATE TABLE ingress_route_generations (
    target_id TEXT PRIMARY KEY NOT NULL,
    generation TEXT NOT NULL CHECK (
        typeof(generation) = 'text'
        AND length(generation) BETWEEN 1 AND 20
        AND generation NOT GLOB '*[^0-9]*'
    ),
    config_digest TEXT,
    updated_at_ms INTEGER NOT NULL
);
INSERT INTO "ingress_route_generations" VALUES('00010000-0000-4000-8000-000000000002','1','cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc',2100);
CREATE TABLE environment_value_overrides (
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    environment TEXT NOT NULL,
    global_key TEXT NOT NULL,
    value TEXT NOT NULL,
    revision INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (workspace_id, environment, global_key)
);
INSERT INTO "environment_value_overrides" VALUES('38fa4aab-5223-4d5d-b886-6efee1b361a6','production','LOG.LEVEL','debug',1,1100);
CREATE TABLE local_project_preferences (
    project_id TEXT PRIMARY KEY NOT NULL,
    runtime TEXT,
    target TEXT,
    updated_at_ms INTEGER NOT NULL
);
INSERT INTO "local_project_preferences" VALUES('00010000-0000-4000-8000-000000000001','docker-compose','local',1000);
CREATE TABLE local_environment_preferences (
    project_id TEXT NOT NULL,
    environment TEXT NOT NULL,
    runtime TEXT,
    target TEXT,
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (project_id, environment)
);
INSERT INTO "local_environment_preferences" VALUES('00010000-0000-4000-8000-000000000001','production','podman-compose',NULL,1000);
CREATE TABLE environment_override_revisions (
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    environment TEXT NOT NULL,
    global_key TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    PRIMARY KEY (workspace_id, environment, global_key)
);
INSERT INTO "environment_override_revisions" VALUES('38fa4aab-5223-4d5d-b886-6efee1b361a6','production','LOG.LEVEL',1);
CREATE TABLE gate_policies (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    environment_name TEXT NOT NULL,
    policy_name TEXT NOT NULL,
    policy_json TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    updated_at_ms INTEGER NOT NULL,
    UNIQUE (project_id, environment_name, policy_name)
);
CREATE TABLE gate_users (
    id TEXT PRIMARY KEY NOT NULL,
    policy_id TEXT NOT NULL REFERENCES gate_policies(id) ON DELETE CASCADE,
    normalized_username TEXT NOT NULL,
    display_username TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    password_parameter_version INTEGER NOT NULL,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    password_version INTEGER NOT NULL CHECK (password_version > 0),
    failed_attempts INTEGER NOT NULL DEFAULT 0 CHECK (failed_attempts >= 0),
    window_started_at_ms INTEGER NOT NULL,
    locked_until_ms INTEGER,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    UNIQUE (policy_id, normalized_username)
);
CREATE TABLE gate_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    policy_id TEXT NOT NULL REFERENCES gate_policies(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES gate_users(id) ON DELETE CASCADE,
    username TEXT NOT NULL,
    issued_at_ms INTEGER NOT NULL,
    expires_at_ms INTEGER NOT NULL,
    last_seen_at_ms INTEGER NOT NULL,
    key_version INTEGER NOT NULL CHECK (key_version > 0),
    revoked_at_ms INTEGER,
    replaced_by TEXT
);
CREATE TABLE gate_route_attachments (
    policy_id TEXT NOT NULL REFERENCES gate_policies(id) ON DELETE CASCADE,
    target_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    environment_name TEXT NOT NULL,
    router_name TEXT NOT NULL,
    host TEXT NOT NULL,
    path_prefix TEXT NOT NULL,
    middleware_name TEXT NOT NULL,
    attached_at_ms INTEGER NOT NULL,
    PRIMARY KEY (target_id, router_name),
    UNIQUE (policy_id, target_id, router_name)
);
CREATE TABLE gate_cap_redemptions (
    token_digest TEXT PRIMARY KEY NOT NULL,
    policy_id TEXT NOT NULL REFERENCES gate_policies(id) ON DELETE CASCADE,
    redeemed_at_ms INTEGER NOT NULL,
    expires_at_ms INTEGER NOT NULL
);
CREATE TABLE gate_audit (
    id TEXT PRIMARY KEY NOT NULL,
    action TEXT NOT NULL,
    project_id TEXT NOT NULL,
    environment_name TEXT NOT NULL,
    policy_name TEXT NOT NULL,
    subject TEXT NOT NULL,
    plan_hash TEXT NOT NULL,
    occurred_at_ms INTEGER NOT NULL
);
CREATE TABLE runtime_exec_audit (
    id TEXT PRIMARY KEY NOT NULL,
    principal TEXT NOT NULL,
    runtime TEXT NOT NULL,
    project_id TEXT NOT NULL,
    environment_name TEXT NOT NULL,
    stack_name TEXT NOT NULL,
    module_name TEXT,
    service_name TEXT NOT NULL,
    container_id TEXT,
    executable TEXT NOT NULL,
    argv_hash TEXT NOT NULL CHECK (length(argv_hash) = 64),
    started_at_ms INTEGER NOT NULL,
    finished_at_ms INTEGER,
    exit_code INTEGER,
    error_code TEXT,
    CHECK (exit_code IS NULL OR error_code IS NULL),
    CHECK (finished_at_ms IS NULL OR finished_at_ms >= started_at_ms)
);
INSERT INTO "runtime_exec_audit" VALUES('00000000-0000-0000-0000-00000000000b','developer','docker-compose','00010000-0000-4000-8000-000000000001','production','app','web','web',NULL,'sh','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',4000,NULL,NULL,NULL);
CREATE TABLE scheduled_jobs (
    project_id TEXT NOT NULL,
    environment_name TEXT NOT NULL,
    job_name TEXT NOT NULL,
    definition_json TEXT NOT NULL,
    definition_hash TEXT NOT NULL CHECK (length(definition_hash) = 64),
    config_hash TEXT NOT NULL CHECK (length(config_hash) = 64),
    retired_at_ms INTEGER,
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (project_id, environment_name, job_name)
);
CREATE TABLE schedules (
    project_id TEXT NOT NULL,
    environment_name TEXT NOT NULL,
    schedule_name TEXT NOT NULL,
    job_name TEXT NOT NULL,
    definition_json TEXT NOT NULL,
    definition_hash TEXT NOT NULL CHECK (length(definition_hash) = 64),
    config_hash TEXT NOT NULL CHECK (length(config_hash) = 64),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    next_due_at_ms INTEGER,
    generation INTEGER NOT NULL DEFAULT 1 CHECK (generation > 0),
    lease_owner TEXT,
    lease_expires_at_ms INTEGER,
    fence_token INTEGER NOT NULL DEFAULT 0 CHECK (fence_token >= 0),
    retired_at_ms INTEGER,
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (project_id, environment_name, schedule_name),
    FOREIGN KEY (project_id, environment_name, job_name)
        REFERENCES scheduled_jobs(project_id, environment_name, job_name)
);
CREATE TABLE job_runs (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    environment_name TEXT NOT NULL,
    job_name TEXT NOT NULL,
    schedule_name TEXT,
    schedule_concurrency TEXT CHECK (schedule_concurrency IN ('allow', 'forbid', 'replace')),
    trigger_kind TEXT NOT NULL,
    idempotency_key TEXT NOT NULL UNIQUE,
    job_definition_json TEXT NOT NULL,
    schedule_definition_json TEXT,
    config_hash TEXT NOT NULL CHECK (length(config_hash) = 64),
    state TEXT NOT NULL,
    attempt INTEGER NOT NULL CHECK (attempt > 0),
    maximum_attempts INTEGER NOT NULL CHECK (maximum_attempts > 0),
    runtime_name TEXT NOT NULL UNIQUE,
    runtime_id TEXT,
    requested_at_ms INTEGER NOT NULL,
    scheduled_for_ms INTEGER,
    started_at_ms INTEGER,
    finished_at_ms INTEGER,
    next_attempt_at_ms INTEGER,
    lease_owner TEXT,
    lease_expires_at_ms INTEGER,
    fence_token INTEGER NOT NULL DEFAULT 0 CHECK (fence_token >= 0),
    cancellation_requested INTEGER NOT NULL DEFAULT 0 CHECK (cancellation_requested IN (0, 1)),
    exit_code INTEGER,
    error_code TEXT,
    diagnostic_code TEXT,
    output_text TEXT NOT NULL DEFAULT '',
    output_truncated INTEGER NOT NULL DEFAULT 0 CHECK (output_truncated IN (0, 1)),
    principal TEXT NOT NULL,
    source TEXT NOT NULL,
    retry_of_run_id TEXT REFERENCES job_runs(id),
    CHECK (finished_at_ms IS NULL OR finished_at_ms >= requested_at_ms),
    CHECK (length(output_text) <= 1048576)
);
CREATE TABLE job_attempts (
    run_id TEXT NOT NULL REFERENCES job_runs(id) ON DELETE CASCADE,
    attempt INTEGER NOT NULL,
    fence_token INTEGER NOT NULL,
    runtime_id TEXT,
    started_at_ms INTEGER NOT NULL,
    finished_at_ms INTEGER,
    exit_code INTEGER,
    error_code TEXT,
    diagnostic_code TEXT,
    PRIMARY KEY (run_id, attempt)
);
CREATE TABLE job_audit (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    occurred_at_ms INTEGER NOT NULL,
    project_id TEXT NOT NULL,
    environment_name TEXT NOT NULL,
    principal TEXT NOT NULL,
    source TEXT NOT NULL,
    action TEXT NOT NULL,
    subject_kind TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    definition_hash TEXT,
    details_json TEXT NOT NULL CHECK (length(details_json) <= 4096)
);
CREATE TABLE mcp_audit (
    id TEXT PRIMARY KEY NOT NULL,
    request_id TEXT NOT NULL,
    principal TEXT NOT NULL,
    source TEXT NOT NULL,
    project_root TEXT NOT NULL,
    environment_name TEXT NOT NULL,
    target_name TEXT NOT NULL,
    operation TEXT NOT NULL,
    mutation INTEGER NOT NULL CHECK (mutation IN (0, 1)),
    plan_hash TEXT,
    outcome_code TEXT NOT NULL,
    occurred_at_ms INTEGER NOT NULL
);
INSERT INTO "mcp_audit" VALUES('00000000-0000-0000-0000-00000000000c','00000000-0000-0000-0000-00000000000d','local-agent','mcp-stdio','/work/web','production','local','deploy_plan',0,'ff2fc70899a737473e78e5eb76125c74112a0a79cbdad55a85833c20a9e02b5f','ok',4100);
CREATE INDEX operations_workspace_requested
    ON operations(workspace_id, requested_at_ms DESC);
CREATE INDEX operation_events_operation
    ON operation_events(operation_id, sequence);
CREATE INDEX ingress_middleware_definitions_owner
    ON ingress_middleware_definitions(target_id, project_id, environment_name);
CREATE INDEX ingress_route_claims_owner
    ON ingress_route_claims(target_id, project_id, environment_name);
CREATE INDEX environment_value_overrides_environment
    ON environment_value_overrides(workspace_id, environment);
CREATE INDEX gate_sessions_policy_expiry
    ON gate_sessions(policy_id, expires_at_ms DESC);
CREATE INDEX gate_audit_owner_time
    ON gate_audit(project_id, environment_name, occurred_at_ms DESC);
CREATE INDEX runtime_exec_audit_owner_time
    ON runtime_exec_audit(project_id, environment_name, started_at_ms DESC);
CREATE INDEX schedules_due
    ON schedules(enabled, retired_at_ms, next_due_at_ms, lease_expires_at_ms);
CREATE INDEX job_runs_owner_time
    ON job_runs(project_id, environment_name, requested_at_ms DESC);
CREATE INDEX job_runs_runnable
    ON job_runs(state, next_attempt_at_ms, lease_expires_at_ms);
CREATE INDEX job_runs_schedule_active
    ON job_runs(project_id, environment_name, schedule_name, state);
CREATE INDEX job_audit_owner_time
    ON job_audit(project_id, environment_name, occurred_at_ms DESC);
CREATE INDEX mcp_audit_scope_time
    ON mcp_audit(project_root, environment_name, target_name, occurred_at_ms DESC);
DELETE FROM sqlite_sequence;
INSERT INTO "sqlite_sequence" VALUES('operation_events',5);
COMMIT;
