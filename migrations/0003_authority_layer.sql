ALTER TABLE permission_grants
    ADD COLUMN effect TEXT NOT NULL DEFAULT 'allow'
    CHECK (effect IN ('allow', 'deny'));
ALTER TABLE permission_grants
    ADD COLUMN lifetime TEXT NOT NULL DEFAULT 'always'
    CHECK (lifetime IN ('once', 'session', 'until', 'always'));
ALTER TABLE permission_grants ADD COLUMN session_id TEXT;
ALTER TABLE permission_grants
    ADD COLUMN granted_by TEXT NOT NULL DEFAULT 'user_explicit'
    CHECK (granted_by IN ('user_explicit', 'system'));
ALTER TABLE permission_grants ADD COLUMN use_limit INTEGER CHECK (use_limit > 0);
ALTER TABLE permission_grants
    ADD COLUMN use_count INTEGER NOT NULL DEFAULT 0 CHECK (use_count >= 0);

CREATE INDEX permission_grants_active
    ON permission_grants(capability_id, revoked_at_ms, expires_at_ms);

CREATE TABLE security_configuration (
    singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
    profile TEXT NOT NULL CHECK (profile IN ('strict', 'balanced', 'developer', 'custom')),
    updated_at_ms INTEGER NOT NULL
);

INSERT INTO security_configuration(singleton_id, profile, updated_at_ms)
VALUES (1, 'balanced', 0);

CREATE TABLE resource_labels (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 128),
    resource_kind TEXT NOT NULL CHECK (resource_kind IN (
        'application', 'file', 'network_endpoint', 'database', 'agent', 'plugin', 'system'
    )),
    match_kind TEXT NOT NULL CHECK (match_kind IN ('exact', 'path_prefix', 'host')),
    pattern TEXT NOT NULL CHECK (length(pattern) BETWEEN 1 AND 1024),
    classification TEXT NOT NULL CHECK (classification IN (
        'public', 'local', 'personal', 'sensitive', 'secret', 'production', 'system_critical'
    )),
    data_classification TEXT NOT NULL CHECK (data_classification IN (
        'public', 'personal', 'sensitive', 'secret', 'production'
    )),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms),
    UNIQUE(resource_kind, match_kind, pattern)
);

CREATE INDEX resource_labels_lookup
    ON resource_labels(resource_kind, match_kind, pattern);

CREATE TABLE intent_usage_events (
    id TEXT PRIMARY KEY,
    intent TEXT NOT NULL CHECK (length(intent) BETWEEN 1 AND 128),
    entity_id TEXT NOT NULL CHECK (length(entity_id) BETWEEN 1 AND 128),
    outcome TEXT NOT NULL CHECK (outcome IN (
        'success', 'failure', 'correction_accepted', 'correction_rejected'
    )),
    context_class TEXT NOT NULL CHECK (length(context_class) BETWEEN 1 AND 128),
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX intent_usage_recent
    ON intent_usage_events(intent, context_class, created_at_ms DESC);

CREATE TABLE agent_configurations (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 128),
    transport TEXT NOT NULL CHECK (transport IN ('cli', 'unix_socket')),
    executable TEXT NOT NULL CHECK (length(executable) <= 1024),
    arguments_json TEXT NOT NULL CHECK (
        json_valid(arguments_json) AND length(arguments_json) <= 32768
    ),
    socket_path TEXT CHECK (length(socket_path) <= 1024),
    sandbox TEXT NOT NULL CHECK (sandbox IN (
        'bubblewrap', 'unavailable', 'unsafe_unsandboxed'
    )),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    timeout_ms INTEGER NOT NULL CHECK (timeout_ms BETWEEN 100 AND 300000),
    max_stdout_bytes INTEGER NOT NULL CHECK (max_stdout_bytes BETWEEN 1024 AND 1048576),
    max_stderr_bytes INTEGER NOT NULL CHECK (max_stderr_bytes BETWEEN 1024 AND 1048576),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms)
);

CREATE INDEX agent_configurations_enabled
    ON agent_configurations(enabled, name COLLATE NOCASE);
