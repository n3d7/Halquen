use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ActivityId, CacheEntryId, ChatMessageId, ChatSessionId, ModelId, ProviderId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppearanceMode {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingPreset {
    Balanced,
    MinimizeAiUsage,
    MinimizeCost,
    PreferLocal,
    PreferQuality,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationSettings {
    pub appearance: AppearanceMode,
    pub language: String,
    pub allow_cloud_ai: bool,
    pub allow_local_ai: bool,
    pub allow_personal_context: bool,
    pub routing_preset: RoutingPreset,
    pub max_model_calls_per_request: u8,
    pub max_context_tokens: u32,
    pub max_output_tokens: u32,
    pub prefer_cached_local: bool,
    pub allow_expensive_fallback: bool,
    pub personal_instructions: String,
    pub learning_enabled: bool,
    pub ask_before_procedural_rules: bool,
    pub auto_save_explicit_preferences: bool,
    pub conversation_retention_days: u16,
    pub episodic_retention_days: u16,
    pub log_level: LogLevel,
    pub diagnostic_logging: bool,
    pub log_retention_days: u16,
    pub log_max_total_mb: u16,
}

impl Default for ApplicationSettings {
    fn default() -> Self {
        Self {
            appearance: AppearanceMode::System,
            language: "system".to_owned(),
            allow_cloud_ai: false,
            allow_local_ai: true,
            allow_personal_context: false,
            routing_preset: RoutingPreset::Balanced,
            max_model_calls_per_request: 1,
            max_context_tokens: 8_192,
            max_output_tokens: 2_048,
            prefer_cached_local: true,
            allow_expensive_fallback: false,
            personal_instructions: String::new(),
            learning_enabled: true,
            ask_before_procedural_rules: true,
            auto_save_explicit_preferences: true,
            conversation_retention_days: 90,
            episodic_retention_days: 30,
            log_level: LogLevel::Info,
            diagnostic_logging: true,
            log_retention_days: 7,
            log_max_total_mb: 32,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SettingsValidationError {
    #[error("language must contain between 1 and 32 characters")]
    InvalidLanguage,
    #[error("personal instructions exceed 8000 bytes")]
    PersonalInstructionsTooLong,
    #[error("model call budget must be between 0 and 3")]
    InvalidModelCallBudget,
    #[error("context token budget must be between 256 and 131072")]
    InvalidContextBudget,
    #[error("output token budget must be between 64 and 16384")]
    InvalidOutputBudget,
    #[error("retention must be between 1 and 3650 days")]
    InvalidRetention,
    #[error("log storage limit must be between 1 and 1024 MiB")]
    InvalidLogLimit,
}

impl ApplicationSettings {
    pub fn validate(&self) -> Result<(), SettingsValidationError> {
        if self.language.trim().is_empty() || self.language.len() > 32 {
            return Err(SettingsValidationError::InvalidLanguage);
        }
        if self.personal_instructions.len() > 8_000 {
            return Err(SettingsValidationError::PersonalInstructionsTooLong);
        }
        if self.max_model_calls_per_request > 3 {
            return Err(SettingsValidationError::InvalidModelCallBudget);
        }
        if !(256..=131_072).contains(&self.max_context_tokens) {
            return Err(SettingsValidationError::InvalidContextBudget);
        }
        if !(64..=16_384).contains(&self.max_output_tokens) {
            return Err(SettingsValidationError::InvalidOutputBudget);
        }
        if !(1..=3_650).contains(&self.conversation_retention_days)
            || !(1..=3_650).contains(&self.episodic_retention_days)
            || !(1..=365).contains(&self.log_retention_days)
        {
            return Err(SettingsValidationError::InvalidRetention);
        }
        if !(1..=1_024).contains(&self.log_max_total_mb) {
            return Err(SettingsValidationError::InvalidLogLimit);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    OpenAiCompatible,
    OpenAi,
    Ollama,
    LmStudio,
    Anthropic,
    Gemini,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyClass {
    Local,
    Cloud,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderStatus {
    Configured,
    Connected,
    Unavailable,
    AuthenticationFailed,
    RateLimited,
    EndpointUnreachable,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provider {
    pub id: ProviderId,
    pub kind: ProviderKind,
    pub name: String,
    pub base_url: String,
    pub enabled: bool,
    pub privacy: PrivacyClass,
    pub configured: bool,
    pub credential_id: Option<String>,
    pub status: ProviderStatus,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiTaskType {
    Conversation,
    MemoryInterpretation,
    Consolidation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiModel {
    pub id: ModelId,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelSelection {
    Automatic,
    Model { model_id: ModelId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatOrigin {
    User,
    Local,
    Cache,
    Ai,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRoute {
    LocalCapability,
    LocalMemory,
    ResponseCache,
    Ai,
    Clarification,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: ChatSessionId,
    pub title: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: ChatMessageId,
    pub session_id: ChatSessionId,
    pub role: ChatRole,
    pub content: String,
    pub origin: ChatOrigin,
    pub route: Option<ChatRoute>,
    pub provider_id: Option<ProviderId>,
    pub model_id: Option<ModelId>,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub latency_ms: Option<u64>,
    pub reusable_candidate_id: Option<CacheEntryId>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    RequestReceived,
    LocalRouteHit,
    LocalRouteMiss,
    CacheHit,
    CacheMiss,
    AiSelected,
    AiCompleted,
    AiFailed,
    MemoryCommitted,
    PolicyEvaluated,
    ExecutionCompleted,
    ConfirmationRequired,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub id: ActivityId,
    pub session_id: Option<ChatSessionId>,
    pub correlation_id: String,
    pub kind: ActivityKind,
    pub summary: String,
    pub detail: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextCategory {
    CurrentRequest,
    PersonalPreference,
    RelevantMemory,
    RecentConversation,
    ExternalUntrusted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseFeedback {
    Useful,
    Wrong,
    DoNotRemember,
    AlwaysUse,
    Prefer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticEntry {
    pub timestamp_ms: i64,
    pub severity: DiagnosticSeverity,
    pub component: String,
    pub code: String,
    pub message: String,
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UsageStats {
    pub model_requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    pub ai_fallbacks: u64,
    pub local_resolutions: u64,
    pub response_cache_hits: u64,
    pub clarifications: u64,
    pub failed_provider_calls: u64,
    pub estimated_tokens_avoided: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_defaults_are_bounded_and_cloud_off() {
        let settings = ApplicationSettings::default();
        assert!(!settings.allow_cloud_ai);
        assert_eq!(settings.max_model_calls_per_request, 1);
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn malformed_budgets_are_rejected() {
        let mut settings = ApplicationSettings::default();
        settings.max_model_calls_per_request = 4;
        assert_eq!(
            settings.validate(),
            Err(SettingsValidationError::InvalidModelCallBudget)
        );
    }
}
