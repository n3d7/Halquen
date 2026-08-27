use halquen_domain::{
    ActivityEvent, AgentConfiguration, AgentId, AgentSession, AiModel, ApplicationSettings,
    CacheEntryId, ChatMessage, ChatSession, ChatSessionId, EntityId, MemoryId, MemoryRevisionId,
    PermissionGrant, PermissionId, Provider, ProviderId, RegisteredApplication, ResourceLabel,
    ResourceLabelId, ResponseFeedback, SecurityProfile, UsageStats,
};
use halquen_memory::{MemoryRevisionView, MemoryView};
use halquen_protocol::{
    AgentConfigurationUpsert, AgentRunRequest, AgentRunResult, ApplicationRegistrationUpsert,
    ChatRequest, ChatResult, ConfirmationPersistence, ConfirmationResult, DaemonClient,
    DiagnosticsSnapshot, MemoryMutationReceipt, MemoryQuery, MemoryStateUpdate, ModelUpsert,
    PermissionGrantUpsert, PromptPreview, ProtocolErrorBody, ProtocolRequest, ProtocolResponse,
    ProviderTestStatus, ProviderUpsert, ResourceLabelUpsert, SecurityOverview,
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandError {
    code: String,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HealthView {
    status: String,
    schema_version: i64,
}

async fn request(request: ProtocolRequest) -> Result<ProtocolResponse, CommandError> {
    let client = DaemonClient::discover().map_err(client_error)?;
    match client.request(request).await.map_err(client_error)? {
        ProtocolResponse::Error { error } => Err(protocol_error(error)),
        response => Ok(response),
    }
}

async fn request_with_id(
    request_id: String,
    request: ProtocolRequest,
) -> Result<ProtocolResponse, CommandError> {
    let client = DaemonClient::discover().map_err(client_error)?;
    match client
        .request_with_id(request_id, request)
        .await
        .map_err(client_error)?
    {
        ProtocolResponse::Error { error } => Err(protocol_error(error)),
        response => Ok(response),
    }
}

fn client_error(_error: impl std::fmt::Display) -> CommandError {
    CommandError {
        code: "daemon_unavailable".to_owned(),
        message: "Halquen daemon is unavailable or uses an incompatible protocol.".to_owned(),
    }
}

fn protocol_error(error: ProtocolErrorBody) -> CommandError {
    CommandError {
        code: format!("{:?}", error.code).to_lowercase(),
        message: error.message,
    }
}

fn unexpected() -> CommandError {
    CommandError {
        code: "unexpected_response".to_owned(),
        message: "The daemon returned an unexpected response type.".to_owned(),
    }
}

#[tauri::command]
pub async fn get_health() -> Result<HealthView, CommandError> {
    match request(ProtocolRequest::Health).await? {
        ProtocolResponse::Health {
            status,
            schema_version,
        } => Ok(HealthView {
            status: format!("{status:?}").to_lowercase(),
            schema_version,
        }),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn send_chat_message(
    request_id: String,
    input: ChatRequest,
) -> Result<ChatResult, CommandError> {
    match request_with_id(request_id, ProtocolRequest::Chat { request: input }).await? {
        ProtocolResponse::Chat { result } => Ok(result),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn cancel_chat_message(request_id: String) -> Result<bool, CommandError> {
    match request(ProtocolRequest::CancelChat { request_id }).await? {
        ProtocolResponse::ChatCancellation { requested } => Ok(requested),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn list_chat_sessions(limit: u16) -> Result<Vec<ChatSession>, CommandError> {
    match request(ProtocolRequest::ListChatSessions { limit }).await? {
        ProtocolResponse::ChatSessions { sessions } => Ok(sessions),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn list_chat_messages(
    session_id: ChatSessionId,
    limit: u16,
) -> Result<Vec<ChatMessage>, CommandError> {
    match request(ProtocolRequest::ListChatMessages { session_id, limit }).await? {
        ProtocolResponse::ChatMessages { messages } => Ok(messages),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn list_activity(limit: u16) -> Result<Vec<ActivityEvent>, CommandError> {
    match request(ProtocolRequest::ListActivity { limit }).await? {
        ProtocolResponse::Activity { events } => Ok(events),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn list_memory(query: MemoryQuery) -> Result<Vec<MemoryView>, CommandError> {
    match request(ProtocolRequest::ListMemory { query }).await? {
        ProtocolResponse::MemoryItems { items } => Ok(items),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn get_memory_history(
    memory_id: MemoryId,
) -> Result<Vec<MemoryRevisionView>, CommandError> {
    match request(ProtocolRequest::GetMemoryHistory { memory_id }).await? {
        ProtocolResponse::MemoryHistory { revisions } => Ok(revisions),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn update_memory_state(update: MemoryStateUpdate) -> Result<bool, CommandError> {
    match request(ProtocolRequest::UpdateMemoryState { update }).await? {
        ProtocolResponse::MemoryUpdated { updated } => Ok(updated),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn restore_memory_revision(
    memory_id: MemoryId,
    revision_id: MemoryRevisionId,
) -> Result<MemoryMutationReceipt, CommandError> {
    match request(ProtocolRequest::RestoreMemoryRevision {
        memory_id,
        revision_id,
    })
    .await?
    {
        ProtocolResponse::MemoryMutation { receipt } => Ok(receipt),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn list_providers() -> Result<Vec<Provider>, CommandError> {
    match request(ProtocolRequest::ListProviders).await? {
        ProtocolResponse::Providers { providers } => Ok(providers),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn upsert_provider(input: ProviderUpsert) -> Result<Provider, CommandError> {
    match request(ProtocolRequest::UpsertProvider { provider: input }).await? {
        ProtocolResponse::ProviderSaved { provider } => Ok(provider),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn remove_provider(provider_id: ProviderId) -> Result<bool, CommandError> {
    match request(ProtocolRequest::RemoveProvider { provider_id }).await? {
        ProtocolResponse::ProviderRemoved { removed } => Ok(removed),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn test_provider(provider_id: ProviderId) -> Result<ProviderTestStatus, CommandError> {
    match request(ProtocolRequest::TestProvider { provider_id }).await? {
        ProtocolResponse::ProviderTest { result } => Ok(result),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn list_models() -> Result<Vec<AiModel>, CommandError> {
    match request(ProtocolRequest::ListModels).await? {
        ProtocolResponse::Models { models } => Ok(models),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn upsert_model(input: ModelUpsert) -> Result<AiModel, CommandError> {
    match request(ProtocolRequest::UpsertModel { model: input }).await? {
        ProtocolResponse::ModelSaved { model } => Ok(model),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn get_application_settings() -> Result<ApplicationSettings, CommandError> {
    match request(ProtocolRequest::GetApplicationSettings).await? {
        ProtocolResponse::ApplicationSettings { settings } => Ok(settings),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn update_application_settings(
    settings: ApplicationSettings,
) -> Result<ApplicationSettings, CommandError> {
    match request(ProtocolRequest::UpdateApplicationSettings { settings }).await? {
        ProtocolResponse::SettingsUpdated { settings } => Ok(settings),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn get_usage_stats() -> Result<UsageStats, CommandError> {
    match request(ProtocolRequest::GetUsageStats).await? {
        ProtocolResponse::UsageStats { stats } => Ok(stats),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn get_diagnostics(limit: u16) -> Result<DiagnosticsSnapshot, CommandError> {
    match request(ProtocolRequest::GetDiagnostics { limit }).await? {
        ProtocolResponse::Diagnostics { snapshot } => Ok(snapshot),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn clear_operational_logs() -> Result<u64, CommandError> {
    match request(ProtocolRequest::ClearOperationalLogs).await? {
        ProtocolResponse::OperationalLogsCleared { removed } => Ok(removed),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn submit_response_feedback(
    cache_entry_id: CacheEntryId,
    feedback: ResponseFeedback,
) -> Result<(), CommandError> {
    match request(ProtocolRequest::SubmitResponseFeedback {
        cache_entry_id,
        feedback,
    })
    .await?
    {
        ProtocolResponse::FeedbackRecorded => Ok(()),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn confirm_action(
    confirmation_id: String,
    allow: bool,
    persistence: ConfirmationPersistence,
    expires_at_ms: Option<i64>,
) -> Result<ConfirmationResult, CommandError> {
    match request(ProtocolRequest::ConfirmAction {
        confirmation_id,
        allow,
        persistence,
        expires_at_ms,
    })
    .await?
    {
        ProtocolResponse::Confirmation { result } => Ok(result),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn preview_ai_request(input: ChatRequest) -> Result<PromptPreview, CommandError> {
    match request(ProtocolRequest::PreviewAiRequest { request: input }).await? {
        ProtocolResponse::AiRequestPreview { preview } => Ok(preview),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn get_security_overview() -> Result<SecurityOverview, CommandError> {
    match request(ProtocolRequest::GetSecurityOverview).await? {
        ProtocolResponse::SecurityOverview { overview } => Ok(overview),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn update_security_profile(
    profile: SecurityProfile,
) -> Result<SecurityProfile, CommandError> {
    match request(ProtocolRequest::UpdateSecurityProfile { profile }).await? {
        ProtocolResponse::SecurityProfileUpdated { profile } => Ok(profile),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn list_permission_grants(limit: u16) -> Result<Vec<PermissionGrant>, CommandError> {
    match request(ProtocolRequest::ListPermissionGrants { limit }).await? {
        ProtocolResponse::PermissionGrants { grants } => Ok(grants),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn upsert_permission_grant(
    input: PermissionGrantUpsert,
) -> Result<PermissionGrant, CommandError> {
    match request(ProtocolRequest::UpsertPermissionGrant { grant: input }).await? {
        ProtocolResponse::PermissionSaved { grant } => Ok(grant),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn revoke_permission_grant(permission_id: PermissionId) -> Result<bool, CommandError> {
    match request(ProtocolRequest::RevokePermissionGrant { permission_id }).await? {
        ProtocolResponse::PermissionRevoked { revoked } => Ok(revoked),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn list_resource_labels(limit: u16) -> Result<Vec<ResourceLabel>, CommandError> {
    match request(ProtocolRequest::ListResourceLabels { limit }).await? {
        ProtocolResponse::ResourceLabels { labels } => Ok(labels),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn upsert_resource_label(
    input: ResourceLabelUpsert,
) -> Result<ResourceLabel, CommandError> {
    match request(ProtocolRequest::UpsertResourceLabel { label: input }).await? {
        ProtocolResponse::ResourceLabelSaved { label } => Ok(label),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn remove_resource_label(
    resource_label_id: ResourceLabelId,
) -> Result<bool, CommandError> {
    match request(ProtocolRequest::RemoveResourceLabel { resource_label_id }).await? {
        ProtocolResponse::ResourceLabelRemoved { removed } => Ok(removed),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn list_agents(limit: u16) -> Result<Vec<AgentConfiguration>, CommandError> {
    match request(ProtocolRequest::ListAgents { limit }).await? {
        ProtocolResponse::Agents { agents } => Ok(agents),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn upsert_agent(
    input: AgentConfigurationUpsert,
) -> Result<AgentConfiguration, CommandError> {
    match request(ProtocolRequest::UpsertAgent { agent: input }).await? {
        ProtocolResponse::AgentSaved { agent } => Ok(agent),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn remove_agent(agent_id: AgentId) -> Result<bool, CommandError> {
    match request(ProtocolRequest::RemoveAgent { agent_id }).await? {
        ProtocolResponse::AgentRemoved { removed } => Ok(removed),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn run_agent(input: AgentRunRequest) -> Result<AgentRunResult, CommandError> {
    match request(ProtocolRequest::RunAgent { request: input }).await? {
        ProtocolResponse::AgentRun { result } => Ok(result),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn list_agent_sessions(limit: u16) -> Result<Vec<AgentSession>, CommandError> {
    match request(ProtocolRequest::ListAgentSessions { limit }).await? {
        ProtocolResponse::AgentSessions { sessions } => Ok(sessions),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn list_registered_applications(
    limit: u16,
) -> Result<Vec<RegisteredApplication>, CommandError> {
    match request(ProtocolRequest::ListRegisteredApplications { limit }).await? {
        ProtocolResponse::RegisteredApplications { applications } => Ok(applications),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn upsert_registered_application(
    input: ApplicationRegistrationUpsert,
) -> Result<RegisteredApplication, CommandError> {
    match request(ProtocolRequest::UpsertRegisteredApplication { application: input }).await? {
        ProtocolResponse::RegisteredApplicationSaved { application } => Ok(application),
        _ => Err(unexpected()),
    }
}

#[tauri::command]
pub async fn remove_registered_application(entity_id: EntityId) -> Result<bool, CommandError> {
    match request(ProtocolRequest::RemoveRegisteredApplication { entity_id }).await? {
        ProtocolResponse::RegisteredApplicationRemoved { removed } => Ok(removed),
        _ => Err(unexpected()),
    }
}
