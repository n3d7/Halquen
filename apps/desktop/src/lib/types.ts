export type Id = string;

export type Health = { status: string; schema_version: number };

export type ModelSelection = { kind: "automatic" } | { kind: "model"; model_id: Id };

export type ChatRequest = {
  session_id: Id | null;
  message: string;
  model_selection: ModelSelection;
};

export type ChatRoute =
  | "local_capability"
  | "local_memory"
  | "response_cache"
  | "ai"
  | "clarification"
  | "unavailable";

export type ChatMessage = {
  id: Id;
  session_id: Id;
  role: "user" | "assistant" | "system";
  content: string;
  origin: "user" | "local" | "cache" | "ai" | "system";
  route: ChatRoute | null;
  provider_id: Id | null;
  model_id: Id | null;
  input_tokens: number | null;
  output_tokens: number | null;
  latency_ms: number | null;
  reusable_candidate_id: Id | null;
  created_at_ms: number;
};

export type ChatSession = {
  id: Id;
  title: string;
  created_at_ms: number;
  updated_at_ms: number;
};

export type ConfirmationPrompt = {
  confirmation_id: string;
  title: string;
  reason: string;
  expires_at_ms: number;
  operation: string;
  target: string;
  destination: DestinationClass | null;
  origin: ActionOrigin;
  resource_classifications: ResourceClassification[];
  agent_id: Id | null;
  agent_session_id: Id | null;
};

export type ActionOrigin =
  | "user_explicit" | "local_resolver" | "ai_proposal" | "external_content"
  | "plugin" | "stored_procedure" | "agent" | "system";
export type ResourceClassification =
  | "public" | "local" | "personal" | "sensitive" | "secret" | "production" | "system_critical";
export type DataClassification = "public" | "personal" | "sensitive" | "secret" | "production";
export type DestinationClass = "local" | "trusted_endpoint" | "external" | "untrusted_external";
export type ResourceKind = "application" | "file" | "network_endpoint" | "database" | "agent" | "plugin" | "system";
export type ResourceDescriptor = { kind: ResourceKind; identifier: string; classification: ResourceClassification };
export type ActionArguments = { kind: "none" } | { kind: "open_app"; app: Id };
export type SecurityProfile = "strict" | "balanced" | "developer" | "custom";
export type PermissionEffect = "allow" | "deny";
export type PermissionLifetime = "once" | "session" | "until" | "always";
export type PermissionSessionScope =
  | { kind: "chat"; id: Id }
  | { kind: "agent"; id: Id }
  | { kind: "daemon"; id: Id };
export type PermissionGrant = {
  id: Id;
  effect: PermissionEffect;
  lifetime: PermissionLifetime;
  scope: {
    capability_id: string;
    arguments: ActionArguments;
    resources: ResourceDescriptor[];
    destination: DestinationClass | null;
  };
  session: PermissionSessionScope | null;
  agent_id: Id | null;
  granted_by: ActionOrigin;
  granted_at_ms: number;
  expires_at_ms: number | null;
  revoked_at_ms: number | null;
  use_limit: number | null;
  use_count: number;
};
export type PermissionGrantUpsert = {
  id: Id | null;
  effect: PermissionEffect;
  lifetime: PermissionLifetime;
  capability_id: string;
  arguments: ActionArguments;
  resources: ResourceDescriptor[];
  destination: DestinationClass | null;
  session: PermissionSessionScope | null;
  agent_id: Id | null;
  expires_at_ms: number | null;
};
export type ResourceLabel = {
  id: Id;
  name: string;
  resource_kind: ResourceKind;
  match_kind: "exact" | "path_prefix" | "host";
  pattern: string;
  classification: ResourceClassification;
  data_classification: DataClassification;
  created_at_ms: number;
  updated_at_ms: number;
};
export type ResourceLabelUpsert = Omit<ResourceLabel, "id" | "created_at_ms" | "updated_at_ms"> & { id: Id | null };
export type AgentConfiguration = {
  id: Id;
  name: string;
  transport: "cli" | "unix_socket";
  executable: string;
  arguments: string[];
  socket_path: string | null;
  sandbox: "bubblewrap" | "unavailable" | "unsafe_unsandboxed";
  ownership: "root_only" | "root_or_current_user";
  executable_identity: {
    canonical_path: string;
    device: number;
    inode: number;
    owner_uid: number;
    size: number;
    modified_seconds: number;
    modified_nanoseconds: number;
    sha256_hex: string | null;
  } | null;
  resource_limits: AgentResourceLimits;
  enabled: boolean;
  timeout_ms: number;
  max_stdout_bytes: number;
  max_stderr_bytes: number;
  created_at_ms: number;
  updated_at_ms: number;
};
export type AgentResourceLimits = {
  cpu_seconds: number;
  memory_bytes: number;
  process_count: number;
  file_size_bytes: number;
  open_files: number;
  temp_bytes: number;
};
export type AgentConfigurationUpsert = Omit<
  AgentConfiguration,
  "id" | "created_at_ms" | "updated_at_ms" | "executable_identity"
> & { id: Id | null; sha256_hex: string | null };
export type AgentSession = {
  id: Id;
  agent_id: Id;
  instance_id: Id;
  daemon_session_id: Id;
  status: "running" | "completed" | "failed" | "timed_out" | "crashed";
  started_at_ms: number;
  ended_at_ms: number | null;
};
export type AgentProposalResult = {
  index: number;
  capability_id: Id;
  disposition: "executed" | "simulated" | "confirmation_required" | "denied" | "failed";
  execution_id: Id | null;
  confirmation: ConfirmationPrompt | null;
  message: string;
};
export type AgentRunResult = {
  session: AgentSession;
  message: string;
  proposals: AgentProposalResult[];
  stderr_bytes: number;
};
export type RegisteredApplication = {
  entity_id: Id;
  display_name: string;
  executable: string;
  arguments: string[];
  ownership: "root_only" | "root_or_current_user";
  identity: AgentConfiguration["executable_identity"];
  enabled: boolean;
  created_at_ms: number;
  updated_at_ms: number;
};
export type ApplicationRegistrationUpsert = {
  entity_id: Id;
  display_name: string;
  executable: string;
  arguments: string[];
  ownership: "root_only" | "root_or_current_user";
  sha256_hex: string | null;
  enabled: boolean;
};
export type SecurityOverview = {
  profile: SecurityProfile;
  immutable_rule_ids: string[];
  active_permissions: number;
  resource_labels: number;
  configured_agents: number;
  active_agent_sessions: number;
  registered_applications: number;
};

export type ChatResult = {
  session: ChatSession;
  user_message: ChatMessage;
  assistant_message: ChatMessage;
  confirmation: ConfirmationPrompt | null;
};

export type ActivityKind =
  | "request_received"
  | "local_route_hit"
  | "local_route_miss"
  | "cache_hit"
  | "cache_miss"
  | "ai_selected"
  | "ai_completed"
  | "ai_failed"
  | "memory_committed"
  | "policy_evaluated"
  | "execution_completed"
  | "confirmation_required"
  | "error";

export type ActivityEvent = {
  id: Id;
  session_id: Id | null;
  correlation_id: string;
  kind: ActivityKind;
  summary: string;
  detail: string | null;
  created_at_ms: number;
};

export type TrustClass =
  | "user_explicit"
  | "local_verified"
  | "user_confirmed_result"
  | "user_behaviour"
  | "ai_inferred"
  | "plugin_asserted"
  | "external_content";

export type MemoryValue =
  | { kind: "fact"; subject: Id; predicate: string; object: string }
  | { kind: "relation"; from: Id; relation: string; to: Id }
  | { kind: "preference"; key: string; value: string }
  | { kind: "procedure"; name: string; capability_ids: string[] };

export type MemoryRevision = {
  id: Id;
  memory_id: Id;
  previous_revision_id: Id | null;
  value: MemoryValue;
  evidence_ids: Id[];
  created_at_ms: number;
  valid_from_ms: number | null;
  valid_until_ms: number | null;
};

export type MemoryView = {
  item: {
    id: Id;
    kind: "semantic" | "procedural";
    current_revision_id: Id;
    created_at_ms: number;
    updated_at_ms: number;
  };
  current: MemoryRevision;
  evidence_count: number;
  trust_classes: TrustClass[];
  priority_permille: number;
  confidence_permille: number;
  pinned: boolean;
  disabled: boolean;
  last_used_at_ms: number | null;
};

export type MemoryRevisionView = { revision: MemoryRevision; trust_classes: TrustClass[] };

export type ProviderKind =
  | "open_ai_compatible"
  | "open_ai"
  | "ollama"
  | "lm_studio"
  | "anthropic"
  | "gemini";
export type PrivacyClass = "local" | "cloud";
export type ProviderStatus =
  | "configured"
  | "connected"
  | "unavailable"
  | "authentication_failed"
  | "rate_limited"
  | "endpoint_unreachable"
  | "unsupported";

export type Provider = {
  id: Id;
  kind: ProviderKind;
  name: string;
  base_url: string;
  enabled: boolean;
  privacy: PrivacyClass;
  configured: boolean;
  credential_id: string | null;
  status: ProviderStatus;
  created_at_ms: number;
  updated_at_ms: number;
};

export type ProviderUpsert = {
  id: Id | null;
  kind: ProviderKind;
  name: string;
  base_url: string;
  enabled: boolean;
  privacy: PrivacyClass;
  api_key: string | null;
  clear_api_key: boolean;
};

export type AiTaskType = "conversation" | "memory_interpretation" | "consolidation";
export type AiModel = {
  id: Id;
  provider_id: Id;
  display_name: string;
  provider_model_id: string;
  enabled: boolean;
  context_limit: number | null;
  privacy: PrivacyClass;
  priority: number;
  task_eligibility: AiTaskType[];
  is_default: boolean;
};

export type ModelUpsert = Omit<AiModel, "id"> & { id: Id | null };

export type ApplicationSettings = {
  appearance: "system" | "light" | "dark";
  language: string;
  allow_cloud_ai: boolean;
  allow_local_ai: boolean;
  allow_personal_context: boolean;
  routing_preset:
    | "balanced"
    | "minimize_ai_usage"
    | "minimize_cost"
    | "prefer_local"
    | "prefer_quality"
    | "custom";
  max_model_calls_per_request: number;
  max_context_tokens: number;
  max_output_tokens: number;
  prefer_cached_local: boolean;
  allow_expensive_fallback: boolean;
  personal_instructions: string;
  learning_enabled: boolean;
  ask_before_procedural_rules: boolean;
  auto_save_explicit_preferences: boolean;
  conversation_retention_days: number;
  episodic_retention_days: number;
  log_level: "error" | "warn" | "info" | "debug";
  diagnostic_logging: boolean;
  log_retention_days: number;
  log_max_total_mb: number;
};

export type UsageStats = {
  model_requests: number;
  input_tokens: number;
  output_tokens: number;
  cached_tokens: number;
  ai_fallbacks: number;
  local_resolutions: number;
  response_cache_hits: number;
  clarifications: number;
  failed_provider_calls: number;
  estimated_tokens_avoided: number;
};

export type DiagnosticEntry = {
  timestamp_ms: number;
  severity: "error" | "warn" | "info" | "debug";
  component: string;
  code: string;
  message: string;
  correlation_id: string | null;
};

export type DiagnosticsSnapshot = {
  protocol_version: number;
  schema_version: number;
  database_path: string;
  runtime_socket: string;
  provider_statuses: Array<{ provider_id: Id; status: ProviderStatus; message: string }>;
  recent: DiagnosticEntry[];
  memory_items: number;
  cached_responses: number;
  unknown_cases: number;
  audit_records: number;
};

export type PromptPreview = {
  provider_id: Id | null;
  model_id: Id | null;
  task: AiTaskType;
  estimated_context_tokens: number;
  context_categories: string[];
  personal_instructions: string;
  core_contract_managed: boolean;
};

export type CommandError = { code: string; message: string };
