use halquen_domain::{
    ActionRequest, AgentId, ApplicationSettings, CacheEntryId, CapabilityId, ChatSessionId,
    EntityId, MemoryId, MemoryRevisionId, PermissionId, ProviderId, ResourceLabelId,
    ResponseFeedback, SecurityProfile,
};
use serde::{Deserialize, Serialize};

use crate::{
    AgentConfigurationUpsert, AgentRunRequest, ApplicationRegistrationUpsert, ChatRequest,
    ConfirmationPersistence, MemoryQuery, MemoryStateUpdate, ModelUpsert, PermissionGrantUpsert,
    ProviderUpsert, ResourceLabelUpsert,
};

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
    GetCapability {
        capability_id: CapabilityId,
    },
    EvaluateAction {
        action: ActionRequest,
    },
    DryRunAction {
        action: ActionRequest,
    },
    ExecuteAction {
        action: ActionRequest,
    },
    MemoryStats,
    AuditStats,
    Chat {
        request: ChatRequest,
    },
    CancelChat {
        request_id: String,
    },
    ListChatSessions {
        limit: u16,
    },
    ListChatMessages {
        session_id: ChatSessionId,
        limit: u16,
    },
    ListActivity {
        limit: u16,
    },
    ListMemory {
        query: MemoryQuery,
    },
    GetMemoryHistory {
        memory_id: MemoryId,
    },
    UpdateMemoryState {
        update: MemoryStateUpdate,
    },
    RestoreMemoryRevision {
        memory_id: MemoryId,
        revision_id: MemoryRevisionId,
    },
    ListProviders,
    UpsertProvider {
        provider: ProviderUpsert,
    },
    RemoveProvider {
        provider_id: ProviderId,
    },
    TestProvider {
        provider_id: ProviderId,
    },
    ListModels,
    UpsertModel {
        model: ModelUpsert,
    },
    GetApplicationSettings,
    UpdateApplicationSettings {
        settings: ApplicationSettings,
    },
    GetUsageStats,
    GetDiagnostics {
        limit: u16,
    },
    ClearOperationalLogs,
    SubmitResponseFeedback {
        cache_entry_id: CacheEntryId,
        feedback: ResponseFeedback,
    },
    ConfirmAction {
        confirmation_id: String,
        allow: bool,
        persistence: ConfirmationPersistence,
        expires_at_ms: Option<i64>,
    },
    PreviewAiRequest {
        request: ChatRequest,
    },
    GetSecurityOverview,
    UpdateSecurityProfile {
        profile: SecurityProfile,
    },
    ListPermissionGrants {
        limit: u16,
    },
    UpsertPermissionGrant {
        grant: PermissionGrantUpsert,
    },
    RevokePermissionGrant {
        permission_id: PermissionId,
    },
    ListResourceLabels {
        limit: u16,
    },
    UpsertResourceLabel {
        label: ResourceLabelUpsert,
    },
    RemoveResourceLabel {
        resource_label_id: ResourceLabelId,
    },
    ListAgents {
        limit: u16,
    },
    UpsertAgent {
        agent: AgentConfigurationUpsert,
    },
    RemoveAgent {
        agent_id: AgentId,
    },
    RunAgent {
        request: AgentRunRequest,
    },
    ListAgentSessions {
        limit: u16,
    },
    ListRegisteredApplications {
        limit: u16,
    },
    UpsertRegisteredApplication {
        application: ApplicationRegistrationUpsert,
    },
    RemoveRegisteredApplication {
        entity_id: EntityId,
    },
}
