ALTER TABLE memory_items
    ADD COLUMN priority_permille INTEGER NOT NULL DEFAULT 500
    CHECK (priority_permille BETWEEN 0 AND 1000);
ALTER TABLE memory_items
    ADD COLUMN confidence_permille INTEGER NOT NULL DEFAULT 1000
    CHECK (confidence_permille BETWEEN 0 AND 1000);
ALTER TABLE memory_items
    ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0, 1));
ALTER TABLE memory_items
    ADD COLUMN disabled INTEGER NOT NULL DEFAULT 0 CHECK (disabled IN (0, 1));
ALTER TABLE memory_items ADD COLUMN last_used_at_ms INTEGER;

CREATE TABLE application_settings (
    singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
    appearance TEXT NOT NULL CHECK (appearance IN ('system', 'light', 'dark')),
    language TEXT NOT NULL CHECK (length(language) BETWEEN 1 AND 32),
    allow_cloud_ai INTEGER NOT NULL CHECK (allow_cloud_ai IN (0, 1)),
    allow_local_ai INTEGER NOT NULL CHECK (allow_local_ai IN (0, 1)),
    allow_personal_context INTEGER NOT NULL CHECK (allow_personal_context IN (0, 1)),
    routing_preset TEXT NOT NULL CHECK (routing_preset IN (
        'balanced', 'minimize_ai_usage', 'minimize_cost', 'prefer_local', 'prefer_quality', 'custom'
    )),
    max_model_calls_per_request INTEGER NOT NULL CHECK (max_model_calls_per_request BETWEEN 0 AND 3),
    max_context_tokens INTEGER NOT NULL CHECK (max_context_tokens BETWEEN 256 AND 131072),
    max_output_tokens INTEGER NOT NULL CHECK (max_output_tokens BETWEEN 64 AND 16384),
    prefer_cached_local INTEGER NOT NULL CHECK (prefer_cached_local IN (0, 1)),
    allow_expensive_fallback INTEGER NOT NULL CHECK (allow_expensive_fallback IN (0, 1)),
    personal_instructions TEXT NOT NULL CHECK (length(personal_instructions) <= 8000),
    learning_enabled INTEGER NOT NULL CHECK (learning_enabled IN (0, 1)),
    ask_before_procedural_rules INTEGER NOT NULL CHECK (ask_before_procedural_rules IN (0, 1)),
    auto_save_explicit_preferences INTEGER NOT NULL CHECK (auto_save_explicit_preferences IN (0, 1)),
    conversation_retention_days INTEGER NOT NULL CHECK (conversation_retention_days BETWEEN 1 AND 3650),
    episodic_retention_days INTEGER NOT NULL CHECK (episodic_retention_days BETWEEN 1 AND 3650),
    log_level TEXT NOT NULL CHECK (log_level IN ('error', 'warn', 'info', 'debug')),
    diagnostic_logging INTEGER NOT NULL CHECK (diagnostic_logging IN (0, 1)),
    log_retention_days INTEGER NOT NULL CHECK (log_retention_days BETWEEN 1 AND 365),
    log_max_total_mb INTEGER NOT NULL CHECK (log_max_total_mb BETWEEN 1 AND 1024),
    updated_at_ms INTEGER NOT NULL
);

INSERT INTO application_settings VALUES (
    1, 'system', 'system', 0, 1, 0, 'balanced', 1, 8192, 2048, 1, 0, '',
    1, 1, 1, 90, 30, 'info', 1, 7, 32, 0
);

CREATE TABLE ai_providers (
    id TEXT PRIMARY KEY,
    provider_kind TEXT NOT NULL CHECK (provider_kind IN (
        'open_ai_compatible', 'open_ai', 'ollama', 'lm_studio', 'anthropic', 'gemini'
    )),
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 128),
    base_url TEXT NOT NULL CHECK (length(base_url) BETWEEN 1 AND 2048),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    privacy_class TEXT NOT NULL CHECK (privacy_class IN ('local', 'cloud')),
    credential_id TEXT UNIQUE,
    status TEXT NOT NULL CHECK (status IN (
        'configured', 'connected', 'unavailable', 'authentication_failed', 'rate_limited',
        'endpoint_unreachable', 'unsupported'
    )),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE ai_models (
    id TEXT PRIMARY KEY,
    provider_id TEXT NOT NULL REFERENCES ai_providers(id) ON DELETE CASCADE,
    display_name TEXT NOT NULL CHECK (length(display_name) BETWEEN 1 AND 128),
    provider_model_id TEXT NOT NULL CHECK (length(provider_model_id) BETWEEN 1 AND 256),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    context_limit INTEGER CHECK (context_limit BETWEEN 256 AND 1048576),
    privacy_class TEXT NOT NULL CHECK (privacy_class IN ('local', 'cloud')),
    priority INTEGER NOT NULL CHECK (priority BETWEEN -1000 AND 1000),
    is_default INTEGER NOT NULL CHECK (is_default IN (0, 1)),
    UNIQUE(provider_id, provider_model_id)
);

CREATE TABLE ai_model_tasks (
    model_id TEXT NOT NULL REFERENCES ai_models(id) ON DELETE CASCADE,
    task_type TEXT NOT NULL CHECK (task_type IN ('conversation', 'memory_interpretation', 'consolidation')),
    PRIMARY KEY(model_id, task_type)
);

CREATE UNIQUE INDEX ai_models_one_default
    ON ai_models(provider_id) WHERE is_default = 1;

CREATE TABLE chat_sessions (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 256),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE chat_messages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
    content TEXT NOT NULL CHECK (length(content) BETWEEN 1 AND 65536),
    origin TEXT NOT NULL CHECK (origin IN ('user', 'local', 'cache', 'ai', 'system')),
    route TEXT CHECK (route IN (
        'local_capability', 'local_memory', 'response_cache', 'ai', 'clarification', 'unavailable'
    )),
    provider_id TEXT REFERENCES ai_providers(id) ON DELETE SET NULL,
    model_id TEXT REFERENCES ai_models(id) ON DELETE SET NULL,
    input_tokens INTEGER CHECK (input_tokens >= 0),
    output_tokens INTEGER CHECK (output_tokens >= 0),
    latency_ms INTEGER CHECK (latency_ms >= 0),
    reusable_candidate_id TEXT,
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX chat_messages_session_created
    ON chat_messages(session_id, created_at_ms DESC);

CREATE TABLE response_cache (
    id TEXT PRIMARY KEY,
    normalized_request TEXT NOT NULL CHECK (length(normalized_request) BETWEEN 1 AND 4096),
    response TEXT NOT NULL CHECK (length(response) BETWEEN 1 AND 65536),
    context_key TEXT NOT NULL CHECK (length(context_key) BETWEEN 1 AND 512),
    confidence_permille INTEGER NOT NULL CHECK (confidence_permille BETWEEN 0 AND 1000),
    priority_permille INTEGER NOT NULL CHECK (priority_permille BETWEEN 0 AND 1000),
    trust_class TEXT NOT NULL,
    valid_until_ms INTEGER,
    reusable INTEGER NOT NULL CHECK (reusable IN (0, 1)),
    created_at_ms INTEGER NOT NULL,
    last_used_at_ms INTEGER,
    usage_count INTEGER NOT NULL DEFAULT 0 CHECK (usage_count >= 0),
    success_count INTEGER NOT NULL DEFAULT 0 CHECK (success_count >= 0),
    correction_count INTEGER NOT NULL DEFAULT 0 CHECK (correction_count >= 0),
    original_provider_id TEXT REFERENCES ai_providers(id) ON DELETE SET NULL,
    original_model_id TEXT REFERENCES ai_models(id) ON DELETE SET NULL,
    estimated_tokens_avoided INTEGER NOT NULL DEFAULT 0 CHECK (estimated_tokens_avoided >= 0),
    UNIQUE(normalized_request, context_key)
);

CREATE INDEX response_cache_lookup
    ON response_cache(normalized_request, context_key, reusable, valid_until_ms);

CREATE TABLE activity_events (
    id TEXT PRIMARY KEY,
    session_id TEXT REFERENCES chat_sessions(id) ON DELETE SET NULL,
    correlation_id TEXT NOT NULL CHECK (length(correlation_id) BETWEEN 1 AND 128),
    activity_kind TEXT NOT NULL CHECK (activity_kind IN (
        'request_received', 'local_route_hit', 'local_route_miss', 'cache_hit', 'cache_miss',
        'ai_selected', 'ai_completed', 'ai_failed', 'memory_committed', 'policy_evaluated',
        'execution_completed', 'confirmation_required', 'error'
    )),
    summary TEXT NOT NULL CHECK (length(summary) BETWEEN 1 AND 1024),
    detail TEXT CHECK (length(detail) <= 4096),
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX activity_events_created ON activity_events(created_at_ms DESC);

CREATE TABLE usage_stats (
    singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
    model_requests INTEGER NOT NULL DEFAULT 0 CHECK (model_requests >= 0),
    input_tokens INTEGER NOT NULL DEFAULT 0 CHECK (input_tokens >= 0),
    output_tokens INTEGER NOT NULL DEFAULT 0 CHECK (output_tokens >= 0),
    cached_tokens INTEGER NOT NULL DEFAULT 0 CHECK (cached_tokens >= 0),
    ai_fallbacks INTEGER NOT NULL DEFAULT 0 CHECK (ai_fallbacks >= 0),
    local_resolutions INTEGER NOT NULL DEFAULT 0 CHECK (local_resolutions >= 0),
    response_cache_hits INTEGER NOT NULL DEFAULT 0 CHECK (response_cache_hits >= 0),
    clarifications INTEGER NOT NULL DEFAULT 0 CHECK (clarifications >= 0),
    failed_provider_calls INTEGER NOT NULL DEFAULT 0 CHECK (failed_provider_calls >= 0),
    estimated_tokens_avoided INTEGER NOT NULL DEFAULT 0 CHECK (estimated_tokens_avoided >= 0)
);

INSERT INTO usage_stats(singleton_id) VALUES (1);
