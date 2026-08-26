use halquen_domain::{
    AiModel, AiTaskType, ApplicationSettings, ChatMessage, ChatSession, ContextCategory,
    ExecutionId, MemoryId, MemoryRevisionId, ModelId, ModelSelection, PrivacyClass, ProviderId,
    ProviderKind, ProviderStatus,
};
use halquen_memory::MemoryKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatRequest {
    pub session_id: Option<halquen_domain::ChatSessionId>,
    pub message: String,
    pub model_selection: ModelSelection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatResult {
    pub session: ChatSession,
    pub user_message: ChatMessage,
    pub assistant_message: ChatMessage,
    pub confirmation: Option<ConfirmationPrompt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmationPrompt {
    pub confirmation_id: String,
    pub title: String,
    pub reason: String,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmationResult {
    pub execution_id: Option<ExecutionId>,
    pub accepted: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderUpsert {
    pub id: Option<ProviderId>,
    pub kind: ProviderKind,
    pub name: String,
    pub base_url: String,
    pub enabled: bool,
    pub privacy: PrivacyClass,
    pub api_key: Option<String>,
    pub clear_api_key: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelUpsert {
    pub id: Option<ModelId>,
    pub provider_id: ProviderId,
    pub display_name: String,
    pub provider_model_id: String,
    pub enabled: bool,
    pub context_limit: Option<u32>,
    pub privacy: PrivacyClass,
    pub priority: i16,
    pub task_eligibility: Vec<AiTaskType>,
    pub is_default: bool,
}

impl ModelUpsert {
    pub fn into_model(self) -> AiModel {
        AiModel {
            id: self.id.unwrap_or_else(ModelId::generate),
            provider_id: self.provider_id,
            display_name: self.display_name,
            provider_model_id: self.provider_model_id,
            enabled: self.enabled,
            context_limit: self.context_limit,
            privacy: self.privacy,
            priority: self.priority,
            task_eligibility: self.task_eligibility,
            is_default: self.is_default,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderTestStatus {
    pub provider_id: ProviderId,
    pub status: ProviderStatus,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryQuery {
    pub kind: Option<MemoryKind>,
    pub search: Option<String>,
    pub limit: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryStateUpdate {
    pub memory_id: MemoryId,
    pub pinned: Option<bool>,
    pub disabled: Option<bool>,
    pub priority_permille: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryMutationReceipt {
    pub memory_id: MemoryId,
    pub revision_id: MemoryRevisionId,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptPreview {
    pub provider_id: Option<ProviderId>,
    pub model_id: Option<ModelId>,
    pub task: AiTaskType,
    pub estimated_context_tokens: u32,
    pub context_categories: Vec<ContextCategory>,
    pub personal_instructions: String,
    pub core_contract_managed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsSnapshot {
    pub protocol_version: u16,
    pub schema_version: i64,
    pub database_path: String,
    pub runtime_socket: String,
    pub provider_statuses: Vec<ProviderTestStatus>,
    pub recent: Vec<halquen_domain::DiagnosticEntry>,
    pub memory_items: u64,
    pub cached_responses: u64,
    pub unknown_cases: u64,
    pub audit_records: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsView {
    pub settings: ApplicationSettings,
}
