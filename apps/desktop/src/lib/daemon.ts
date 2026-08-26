import { invoke } from "@tauri-apps/api/core";
import type {
  ActivityEvent,
  AiModel,
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
  PromptPreview,
  Provider,
  ProviderUpsert,
  UsageStats,
} from "./types";

export const daemon = {
  health: () => invoke<Health>("get_health"),
  sendChat: (input: ChatRequest) => invoke<ChatResult>("send_chat_message", { input }),
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
  feedback: (cacheEntryId: Id, feedback: "useful" | "wrong" | "do_not_remember" | "always_use" | "prefer") =>
    invoke<void>("submit_response_feedback", { cacheEntryId, feedback }),
  confirm: (confirmationId: string, allow: boolean) =>
    invoke<{ execution_id: Id | null; accepted: boolean; message: string }>("confirm_action", {
      confirmationId,
      allow,
    }),
  preview: (input: ChatRequest) => invoke<PromptPreview>("preview_ai_request", { input }),
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
