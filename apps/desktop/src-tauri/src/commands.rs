use halquen_domain::{
    ActivityEvent, AiModel, ApplicationSettings, CacheEntryId, ChatMessage, ChatSession,
    ChatSessionId, MemoryId, MemoryRevisionId, Provider, ProviderId, ResponseFeedback, UsageStats,
};
use halquen_memory::{MemoryRevisionView, MemoryView};
use halquen_protocol::{
    ChatRequest, ChatResult, ConfirmationResult, DaemonClient, DiagnosticsSnapshot,
    MemoryMutationReceipt, MemoryQuery, MemoryStateUpdate, ModelUpsert, PromptPreview,
    ProtocolErrorBody, ProtocolRequest, ProtocolResponse, ProviderTestStatus, ProviderUpsert,
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
pub async fn send_chat_message(input: ChatRequest) -> Result<ChatResult, CommandError> {
    match request(ProtocolRequest::Chat { request: input }).await? {
        ProtocolResponse::Chat { result } => Ok(result),
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
) -> Result<ConfirmationResult, CommandError> {
    match request(ProtocolRequest::ConfirmAction {
        confirmation_id,
        allow,
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
