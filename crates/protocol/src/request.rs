use halquen_domain::{
    ActionRequest, ApplicationSettings, CacheEntryId, CapabilityId, ChatSessionId, MemoryId,
    MemoryRevisionId, ProviderId, ResponseFeedback,
};
use serde::{Deserialize, Serialize};

use crate::{ChatRequest, MemoryQuery, MemoryStateUpdate, ModelUpsert, ProviderUpsert};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestEnvelope {
    pub version: u16,
    pub request_id: String,
    pub request: ProtocolRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum ProtocolRequest {
    Health,
    ListCapabilities,
    GetCapability { capability_id: CapabilityId },
    EvaluateAction { action: ActionRequest },
    DryRunAction { action: ActionRequest },
    MemoryStats,
    AuditStats,
    Chat { request: ChatRequest },
    ListChatSessions { limit: u16 },
    ListChatMessages { session_id: ChatSessionId, limit: u16 },
    ListActivity { limit: u16 },
    ListMemory { query: MemoryQuery },
    GetMemoryHistory { memory_id: MemoryId },
    UpdateMemoryState { update: MemoryStateUpdate },
    RestoreMemoryRevision {
        memory_id: MemoryId,
        revision_id: MemoryRevisionId,
    },
    ListProviders,
    UpsertProvider { provider: ProviderUpsert },
    RemoveProvider { provider_id: ProviderId },
    TestProvider { provider_id: ProviderId },
    ListModels,
    UpsertModel { model: ModelUpsert },
    GetApplicationSettings,
    UpdateApplicationSettings { settings: ApplicationSettings },
    GetUsageStats,
    GetDiagnostics { limit: u16 },
    SubmitResponseFeedback {
        cache_entry_id: CacheEntryId,
        feedback: ResponseFeedback,
    },
    ConfirmAction { confirmation_id: String, allow: bool },
    PreviewAiRequest { request: ChatRequest },
}
