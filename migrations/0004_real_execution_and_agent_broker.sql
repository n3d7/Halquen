ALTER TABLE permission_grants
    ADD COLUMN session_kind TEXT
    CHECK (session_kind IN ('chat', 'agent', 'daemon'));
ALTER TABLE permission_grants ADD COLUMN agent_id TEXT;

UPDATE permission_grants
SET session_kind = 'chat'
WHERE session_id IS NOT NULL;

CREATE INDEX permission_grants_agent_scope
    ON permission_grants(agent_id, session_kind, session_id, revoked_at_ms);

ALTER TABLE agent_configurations
    ADD COLUMN executable_ownership TEXT NOT NULL DEFAULT 'root_or_current_user'
    CHECK (executable_ownership IN ('root_only', 'root_or_current_user'));
ALTER TABLE agent_configurations ADD COLUMN executable_device INTEGER;
ALTER TABLE agent_configurations ADD COLUMN executable_inode INTEGER;
ALTER TABLE agent_configurations ADD COLUMN executable_owner_uid INTEGER;
ALTER TABLE agent_configurations ADD COLUMN executable_size INTEGER;
ALTER TABLE agent_configurations ADD COLUMN executable_mtime_seconds INTEGER;
ALTER TABLE agent_configurations ADD COLUMN executable_mtime_nanoseconds INTEGER;
ALTER TABLE agent_configurations ADD COLUMN executable_sha256_hex TEXT
    CHECK (executable_sha256_hex IS NULL OR length(executable_sha256_hex) = 64);
ALTER TABLE agent_configurations
    ADD COLUMN cpu_seconds INTEGER NOT NULL DEFAULT 30 CHECK (cpu_seconds BETWEEN 1 AND 300);
ALTER TABLE agent_configurations
    ADD COLUMN memory_bytes INTEGER NOT NULL DEFAULT 536870912 CHECK (memory_bytes BETWEEN 16777216 AND 8589934592);
ALTER TABLE agent_configurations
    ADD COLUMN process_count INTEGER NOT NULL DEFAULT 64 CHECK (process_count BETWEEN 1 AND 1024);
ALTER TABLE agent_configurations
    ADD COLUMN file_size_bytes INTEGER NOT NULL DEFAULT 16777216 CHECK (file_size_bytes BETWEEN 1024 AND 1073741824);
ALTER TABLE agent_configurations
    ADD COLUMN open_files INTEGER NOT NULL DEFAULT 128 CHECK (open_files BETWEEN 16 AND 4096);
ALTER TABLE agent_configurations
    ADD COLUMN temp_bytes INTEGER NOT NULL DEFAULT 67108864 CHECK (temp_bytes BETWEEN 1048576 AND 1073741824);

CREATE TABLE registered_applications (
    entity_id TEXT PRIMARY KEY CHECK (entity_id LIKE 'app:%' AND length(entity_id) <= 128),
    display_name TEXT NOT NULL CHECK (length(display_name) BETWEEN 1 AND 128),
    executable TEXT NOT NULL UNIQUE CHECK (length(executable) BETWEEN 1 AND 1024),
    arguments_json TEXT NOT NULL CHECK (json_valid(arguments_json) AND length(arguments_json) <= 32768),
    executable_ownership TEXT NOT NULL CHECK (executable_ownership IN ('root_only', 'root_or_current_user')),
    executable_device INTEGER NOT NULL,
    executable_inode INTEGER NOT NULL,
    executable_owner_uid INTEGER NOT NULL,
    executable_size INTEGER NOT NULL,
    executable_mtime_seconds INTEGER NOT NULL,
    executable_mtime_nanoseconds INTEGER NOT NULL CHECK (executable_mtime_nanoseconds BETWEEN 0 AND 999999999),
    executable_sha256_hex TEXT CHECK (executable_sha256_hex IS NULL OR length(executable_sha256_hex) = 64),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms)
);

CREATE TABLE daemon_sessions (
    id TEXT PRIMARY KEY,
    started_at_ms INTEGER NOT NULL,
    ended_at_ms INTEGER CHECK (ended_at_ms IS NULL OR ended_at_ms >= started_at_ms)
);

CREATE TABLE agent_sessions (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    instance_id TEXT NOT NULL UNIQUE,
    daemon_session_id TEXT NOT NULL REFERENCES daemon_sessions(id) ON DELETE RESTRICT,
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed', 'timed_out', 'crashed')),
    started_at_ms INTEGER NOT NULL,
    ended_at_ms INTEGER CHECK (ended_at_ms IS NULL OR ended_at_ms >= started_at_ms)
);

CREATE INDEX agent_sessions_recent
    ON agent_sessions(started_at_ms DESC, id DESC);
CREATE INDEX agent_sessions_active
    ON agent_sessions(agent_id, status, ended_at_ms);
