PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS workspaces (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    root TEXT NOT NULL UNIQUE,
    config_path TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    last_seen_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS operations (
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

CREATE INDEX IF NOT EXISTS operations_workspace_requested
    ON operations(workspace_id, requested_at_ms DESC);

CREATE TABLE IF NOT EXISTS operation_events (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    operation_id TEXT NOT NULL REFERENCES operations(id) ON DELETE CASCADE,
    occurred_at_ms INTEGER NOT NULL,
    payload_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS operation_events_operation
    ON operation_events(operation_id, sequence);

CREATE TABLE IF NOT EXISTS deployment_snapshots (
    operation_id TEXT PRIMARY KEY NOT NULL REFERENCES operations(id) ON DELETE CASCADE,
    desired_json TEXT NOT NULL,
    observed_before_json TEXT NOT NULL,
    observed_after_json TEXT,
    config_hash TEXT NOT NULL,
    rendered_hash TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS image_build_snapshots (
    operation_id TEXT PRIMARY KEY NOT NULL REFERENCES operations(id) ON DELETE CASCADE,
    plan_json TEXT NOT NULL,
    result_json TEXT
);

CREATE TABLE IF NOT EXISTS image_build_unit_cache (
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

CREATE TABLE IF NOT EXISTS target_bootstrap_snapshots (
    operation_id TEXT PRIMARY KEY NOT NULL REFERENCES operations(id) ON DELETE CASCADE,
    plan_json TEXT NOT NULL,
    result_json TEXT
);

CREATE TABLE IF NOT EXISTS resource_locks (
    resource TEXT PRIMARY KEY NOT NULL,
    owner_operation_id TEXT NOT NULL,
    acquired_at_ms INTEGER NOT NULL,
    expires_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS ingress_route_claims (
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

CREATE TABLE IF NOT EXISTS ingress_middleware_definitions (
    target_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    environment_name TEXT NOT NULL,
    namespaced_name TEXT NOT NULL,
    definition_json TEXT NOT NULL,
    definition_digest TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (target_id, namespaced_name)
);

CREATE INDEX IF NOT EXISTS ingress_middleware_definitions_owner
    ON ingress_middleware_definitions(target_id, project_id, environment_name);

CREATE INDEX IF NOT EXISTS ingress_route_claims_owner
    ON ingress_route_claims(target_id, project_id, environment_name);

CREATE TABLE IF NOT EXISTS ingress_route_generations (
    target_id TEXT PRIMARY KEY NOT NULL,
    generation TEXT NOT NULL CHECK (
        typeof(generation) = 'text'
        AND length(generation) BETWEEN 1 AND 20
        AND generation NOT GLOB '*[^0-9]*'
    ),
    config_digest TEXT,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS environment_value_overrides (
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    environment TEXT NOT NULL,
    global_key TEXT NOT NULL,
    value TEXT NOT NULL,
    revision INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (workspace_id, environment, global_key)
);

CREATE INDEX IF NOT EXISTS environment_value_overrides_environment
    ON environment_value_overrides(workspace_id, environment);

CREATE TABLE IF NOT EXISTS local_project_preferences (
    project_id TEXT PRIMARY KEY NOT NULL,
    runtime TEXT,
    target TEXT,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS local_environment_preferences (
    project_id TEXT NOT NULL,
    environment TEXT NOT NULL,
    runtime TEXT,
    target TEXT,
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (project_id, environment)
);

CREATE TABLE IF NOT EXISTS environment_override_revisions (
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    environment TEXT NOT NULL,
    global_key TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    PRIMARY KEY (workspace_id, environment, global_key)
);

CREATE TABLE IF NOT EXISTS gate_policies (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    environment_name TEXT NOT NULL,
    policy_name TEXT NOT NULL,
    policy_json TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    updated_at_ms INTEGER NOT NULL,
    UNIQUE (project_id, environment_name, policy_name)
);

CREATE TABLE IF NOT EXISTS gate_users (
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

CREATE TABLE IF NOT EXISTS gate_sessions (
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

CREATE INDEX IF NOT EXISTS gate_sessions_policy_expiry
    ON gate_sessions(policy_id, expires_at_ms DESC);

CREATE TABLE IF NOT EXISTS gate_route_attachments (
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

CREATE TABLE IF NOT EXISTS gate_cap_redemptions (
    token_digest TEXT PRIMARY KEY NOT NULL,
    policy_id TEXT NOT NULL REFERENCES gate_policies(id) ON DELETE CASCADE,
    redeemed_at_ms INTEGER NOT NULL,
    expires_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS gate_audit (
    id TEXT PRIMARY KEY NOT NULL,
    action TEXT NOT NULL,
    project_id TEXT NOT NULL,
    environment_name TEXT NOT NULL,
    policy_name TEXT NOT NULL,
    subject TEXT NOT NULL,
    plan_hash TEXT NOT NULL,
    occurred_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS gate_audit_owner_time
    ON gate_audit(project_id, environment_name, occurred_at_ms DESC);

CREATE TABLE IF NOT EXISTS runtime_exec_audit (
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

CREATE INDEX IF NOT EXISTS runtime_exec_audit_owner_time
    ON runtime_exec_audit(project_id, environment_name, started_at_ms DESC);

CREATE TABLE IF NOT EXISTS scheduled_jobs (
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

CREATE TABLE IF NOT EXISTS schedules (
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

CREATE INDEX IF NOT EXISTS schedules_due
    ON schedules(enabled, retired_at_ms, next_due_at_ms, lease_expires_at_ms);

CREATE TABLE IF NOT EXISTS job_runs (
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

CREATE INDEX IF NOT EXISTS job_runs_owner_time
    ON job_runs(project_id, environment_name, requested_at_ms DESC);
CREATE INDEX IF NOT EXISTS job_runs_runnable
    ON job_runs(state, next_attempt_at_ms, lease_expires_at_ms);
CREATE INDEX IF NOT EXISTS job_runs_schedule_active
    ON job_runs(project_id, environment_name, schedule_name, state);

CREATE TABLE IF NOT EXISTS job_attempts (
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

CREATE TABLE IF NOT EXISTS job_audit (
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

CREATE INDEX IF NOT EXISTS job_audit_owner_time
    ON job_audit(project_id, environment_name, occurred_at_ms DESC);

CREATE TABLE IF NOT EXISTS mcp_audit (
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

CREATE INDEX IF NOT EXISTS mcp_audit_scope_time
    ON mcp_audit(project_root, environment_name, target_name, occurred_at_ms DESC);
