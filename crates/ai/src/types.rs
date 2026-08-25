use halquen_domain::{AiTaskType, ContextCategory};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiRequest {
    pub task: AiTaskType,
    pub system_prompt: String,
    pub user_message: String,
    pub context: Vec<ContextItemPayload>,
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextItemPayload {
    pub category: ContextCategory,
    pub content: String,
    pub untrusted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AiUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cached_tokens: u32,
    pub reasoning_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiResponse {
    pub content: String,
    pub provider_model_id: String,
    pub usage: AiUsage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderTestResult {
    pub reachable: bool,
    pub sanitized_message: String,
}

#[derive(Debug, Error)]
pub enum AiError {
    #[error("AI is disabled by the current settings")]
    Disabled,
    #[error("no eligible provider/model route is configured")]
    NoEligibleRoute,
    #[error("provider type is not implemented")]
    UnsupportedProvider,
    #[error("provider endpoint is invalid")]
    InvalidEndpoint,
    #[error("provider credential is unavailable")]
    CredentialUnavailable,
    #[error("provider authentication failed")]
    AuthenticationFailed,
    #[error("provider rate limit was reached")]
    RateLimited,
    #[error("provider endpoint is unavailable")]
    EndpointUnavailable,
    #[error("provider response was invalid")]
    InvalidResponse,
    #[error("provider response exceeded the allowed size")]
    ResponseTooLarge,
    #[error("provider request failed")]
    RequestFailed,
}
