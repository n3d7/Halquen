import { invoke } from "@tauri-apps/api/core";
import type {
  ActivityEvent,
  AiModel,
  AgentConfiguration,
  AgentConfigurationUpsert,
  AgentRunResult,
  AgentSession,
  ApplicationRegistrationUpsert,
  ApplicationSettings,
  ChatMessage,
  ChatRequest,
  ChatResult,
  ChatSession,
  DiagnosticsSnapshot,
  Health,
  Id,
  MemoryRevisionView,
  MemoryView,
  ModelUpsert,
  PermissionGrant,
  PermissionGrantUpsert,
  PromptPreview,
  Provider,
  ProviderUpsert,
  ResourceLabel,
  ResourceLabelUpsert,
  RegisteredApplication,
  SecurityOverview,
  SecurityProfile,
  UsageStats,
} from "./types";

export const daemon = {
  health: () => invoke<Health>("get_health"),
  sendChat: (requestId: string, input: ChatRequest) =>
    invoke<ChatResult>("send_chat_message", { requestId, input }),
  cancelChat: (requestId: string) =>
    invoke<boolean>("cancel_chat_message", { requestId }),
  chatSessions: (limit = 100) => invoke<ChatSession[]>("list_chat_sessions", { limit }),
  chatMessages: (sessionId: Id, limit = 200) =>
    invoke<ChatMessage[]>("list_chat_messages", { sessionId, limit }),
  activity: (limit = 200) => invoke<ActivityEvent[]>("list_activity", { limit }),
  memory: (kind: "semantic" | "procedural" | null, search: string, limit = 200) =>
    invoke<MemoryView[]>("list_memory", {
      query: { kind, search: search.trim() || null, limit },
    }),
  memoryHistory: (memoryId: Id) =>
    invoke<MemoryRevisionView[]>("get_memory_history", { memoryId }),
  updateMemoryState: (
    memoryId: Id,
    update: { pinned?: boolean; disabled?: boolean; priority_permille?: number },
  ) =>
    invoke<boolean>("update_memory_state", {
      update: {
        memory_id: memoryId,
        pinned: update.pinned ?? null,
        disabled: update.disabled ?? null,
        priority_permille: update.priority_permille ?? null,
      },
    }),
  restoreMemory: (memoryId: Id, revisionId: Id) =>
    invoke("restore_memory_revision", { memoryId, revisionId }),
  providers: () => invoke<Provider[]>("list_providers"),
  upsertProvider: (input: ProviderUpsert) => invoke<Provider>("upsert_provider", { input }),
  removeProvider: (providerId: Id) => invoke<boolean>("remove_provider", { providerId }),
  testProvider: (providerId: Id) =>
    invoke<{ provider_id: Id; status: string; message: string }>("test_provider", { providerId }),
  models: () => invoke<AiModel[]>("list_models"),
  upsertModel: (input: ModelUpsert) => invoke<AiModel>("upsert_model", { input }),
  settings: () => invoke<ApplicationSettings>("get_application_settings"),
  updateSettings: (settings: ApplicationSettings) =>
    invoke<ApplicationSettings>("update_application_settings", { settings }),
  usage: () => invoke<UsageStats>("get_usage_stats"),
  diagnostics: (limit = 100) => invoke<DiagnosticsSnapshot>("get_diagnostics", { limit }),
  clearOperationalLogs: () => invoke<number>("clear_operational_logs"),
  feedback: (cacheEntryId: Id, feedback: "useful" | "wrong" | "do_not_remember" | "always_use" | "prefer") =>
    invoke<void>("submit_response_feedback", { cacheEntryId, feedback }),
  confirm: (
    confirmationId: string,
    allow: boolean,
    persistence: "once" | "session" | "until" | "always" = "once",
    expiresAtMs: number | null = null,
  ) =>
    invoke<{ execution_id: Id | null; accepted: boolean; message: string }>("confirm_action", {
      confirmationId,
      allow,
      persistence,
      expiresAtMs,
    }),
  preview: (input: ChatRequest) => invoke<PromptPreview>("preview_ai_request", { input }),
  securityOverview: () => invoke<SecurityOverview>("get_security_overview"),
  updateSecurityProfile: (profile: SecurityProfile) =>
    invoke<SecurityProfile>("update_security_profile", { profile }),
  permissions: (limit = 200) => invoke<PermissionGrant[]>("list_permission_grants", { limit }),
  upsertPermission: (input: PermissionGrantUpsert) =>
    invoke<PermissionGrant>("upsert_permission_grant", { input }),
  revokePermission: (permissionId: Id) =>
    invoke<boolean>("revoke_permission_grant", { permissionId }),
  resourceLabels: (limit = 200) => invoke<ResourceLabel[]>("list_resource_labels", { limit }),
  upsertResourceLabel: (input: ResourceLabelUpsert) =>
    invoke<ResourceLabel>("upsert_resource_label", { input }),
  removeResourceLabel: (resourceLabelId: Id) =>
    invoke<boolean>("remove_resource_label", { resourceLabelId }),
  agents: (limit = 100) => invoke<AgentConfiguration[]>("list_agents", { limit }),
  upsertAgent: (input: AgentConfigurationUpsert) =>
    invoke<AgentConfiguration>("upsert_agent", { input }),
  removeAgent: (agentId: Id) => invoke<boolean>("remove_agent", { agentId }),
  runAgent: (agentId: Id, input: string) =>
    invoke<AgentRunResult>("run_agent", { input: { agent_id: agentId, input } }),
  agentSessions: (limit = 100) => invoke<AgentSession[]>("list_agent_sessions", { limit }),
  applications: (limit = 200) =>
    invoke<RegisteredApplication[]>("list_registered_applications", { limit }),
  upsertApplication: (input: ApplicationRegistrationUpsert) =>
    invoke<RegisteredApplication>("upsert_registered_application", { input }),
  removeApplication: (entityId: Id) =>
    invoke<boolean>("remove_registered_application", { entityId }),
};

export function commandMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "code" in error && "message" in error) {
    const { code, message } = error as { code?: unknown; message?: unknown };
    if (
      typeof code === "string"
      && /^[a-z0-9_]{1,64}$/.test(code)
      && typeof message === "string"
      && message.length <= 512
    ) {
      return message;
    }
  }
  return "Halquen couldn't complete this operation.";
}
