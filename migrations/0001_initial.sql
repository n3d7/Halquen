CREATE TABLE entities (
    id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,
    canonical_name TEXT NOT NULL CHECK (length(canonical_name) BETWEEN 1 AND 512),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    valid_from_ms INTEGER,
    valid_until_ms INTEGER,
    CHECK (valid_until_ms IS NULL OR valid_from_ms IS NULL OR valid_until_ms >= valid_from_ms)
);

CREATE TABLE aliases (
    id INTEGER PRIMARY KEY,
    entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    alias TEXT NOT NULL CHECK (length(alias) BETWEEN 1 AND 512),
    trust_class TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    UNIQUE(entity_id, alias)
);

CREATE TABLE intents (
    id TEXT PRIMARY KEY,
    canonical_name TEXT NOT NULL UNIQUE,
    capability_id TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE intent_examples (
    id INTEGER PRIMARY KEY,
    intent_id TEXT NOT NULL REFERENCES intents(id) ON DELETE CASCADE,
    example TEXT NOT NULL CHECK (length(example) BETWEEN 1 AND 2048),
    trust_class TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE TABLE facts (
    id TEXT PRIMARY KEY,
    subject_entity_id TEXT NOT NULL REFERENCES entities(id),
    predicate TEXT NOT NULL,
    object_value TEXT NOT NULL,
    valid_from_ms INTEGER,
    valid_until_ms INTEGER,
    created_at_ms INTEGER NOT NULL
);

CREATE TABLE relations (
    id TEXT PRIMARY KEY,
    from_entity_id TEXT NOT NULL REFERENCES entities(id),
    relation_type TEXT NOT NULL,
    to_entity_id TEXT NOT NULL REFERENCES entities(id),
    valid_from_ms INTEGER,
    valid_until_ms INTEGER,
    created_at_ms INTEGER NOT NULL
);

CREATE TABLE evidence (
    id TEXT PRIMARY KEY,
    trust_class TEXT NOT NULL,
    source_reference TEXT,
    created_at_ms INTEGER NOT NULL
);

CREATE TABLE memory_items (
    id TEXT PRIMARY KEY,
    memory_kind TEXT NOT NULL CHECK (memory_kind IN ('semantic', 'procedural')),
    current_revision_id TEXT,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    FOREIGN KEY(current_revision_id) REFERENCES memory_revisions(id) DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE memory_revisions (
    id TEXT PRIMARY KEY,
    memory_id TEXT NOT NULL REFERENCES memory_items(id) ON DELETE CASCADE,
    previous_revision_id TEXT REFERENCES memory_revisions(id),
    value_json TEXT NOT NULL CHECK (json_valid(value_json)),
    created_at_ms INTEGER NOT NULL,
    valid_from_ms INTEGER,
    valid_until_ms INTEGER,
    CHECK (valid_until_ms IS NULL OR valid_from_ms IS NULL OR valid_until_ms >= valid_from_ms)
);

CREATE TABLE memory_evidence (
    revision_id TEXT NOT NULL REFERENCES memory_revisions(id) ON DELETE CASCADE,
    evidence_id TEXT NOT NULL REFERENCES evidence(id),
    PRIMARY KEY(revision_id, evidence_id)
);

CREATE TABLE events (
    id TEXT PRIMARY KEY,
    event_kind TEXT NOT NULL,
    subject_id TEXT,
    created_at_ms INTEGER NOT NULL
);

CREATE TABLE executions (
    id TEXT PRIMARY KEY,
    capability_id TEXT NOT NULL,
    capability_version INTEGER NOT NULL CHECK (capability_version > 0),
    started_at_ms INTEGER NOT NULL,
    finished_at_ms INTEGER NOT NULL,
    policy_json TEXT NOT NULL CHECK (json_valid(policy_json)),
    status TEXT NOT NULL,
    reversible INTEGER NOT NULL CHECK (reversible IN (0, 1)),
    result_code TEXT,
    error_code TEXT,
    sanitized_error TEXT,
    compensation_reference TEXT,
    CHECK (finished_at_ms >= started_at_ms)
);

CREATE TABLE audit_records (
    id TEXT PRIMARY KEY,
    created_at_ms INTEGER NOT NULL,
    event_json TEXT NOT NULL CHECK (json_valid(event_json))
);

CREATE TABLE corrections (
    id TEXT PRIMARY KEY,
    target_id TEXT NOT NULL,
    correction_summary TEXT NOT NULL CHECK (length(correction_summary) BETWEEN 1 AND 2048),
    created_at_ms INTEGER NOT NULL
);

CREATE TABLE unknown_cases (
    id TEXT PRIMARY KEY,
    request_summary TEXT NOT NULL CHECK (length(request_summary) BETWEEN 1 AND 2048),
    status TEXT NOT NULL CHECK (status IN ('pending', 'resolved', 'dismissed')),
    created_at_ms INTEGER NOT NULL
);

CREATE TABLE ai_proposals (
    id TEXT PRIMARY KEY,
    provider_model TEXT NOT NULL CHECK (length(provider_model) BETWEEN 1 AND 256),
    created_at_ms INTEGER NOT NULL,
    proposal_json TEXT NOT NULL CHECK (json_valid(proposal_json)),
    evidence_ids_json TEXT NOT NULL CHECK (json_valid(evidence_ids_json)),
    status TEXT NOT NULL CHECK (status IN ('pending', 'accepted', 'rejected', 'superseded'))
);

CREATE TABLE permission_grants (
    id TEXT PRIMARY KEY,
    capability_id TEXT NOT NULL,
    scope_json TEXT NOT NULL CHECK (json_valid(scope_json)),
    granted_at_ms INTEGER NOT NULL,
    expires_at_ms INTEGER,
    revoked_at_ms INTEGER
);

CREATE INDEX aliases_lookup ON aliases(alias);
CREATE INDEX facts_subject ON facts(subject_entity_id);
CREATE INDEX relations_from ON relations(from_entity_id);
CREATE INDEX relations_to ON relations(to_entity_id);
CREATE INDEX memory_revisions_item ON memory_revisions(memory_id, created_at_ms);
CREATE INDEX events_created ON events(created_at_ms);
CREATE INDEX executions_created ON executions(started_at_ms);
CREATE INDEX audit_records_created ON audit_records(created_at_ms);
CREATE INDEX unknown_cases_status ON unknown_cases(status, created_at_ms);
