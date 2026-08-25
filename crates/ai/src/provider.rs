use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use halquen_domain::{AiModel, PrivacyClass, Provider, ProviderKind};
use reqwest::{Client, StatusCode, redirect};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{AiError, AiRequest, AiResponse, AiUsage, ProviderTestResult};

const MAX_PROVIDER_RESPONSE_BYTES: usize = 1024 * 1024;

pub type ProviderFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, AiError>> + Send + 'a>>;

pub trait ProviderClient: Send + Sync {
    fn complete<'a>(
        &'a self,
        provider: &'a Provider,
        model: &'a AiModel,
        credential: Option<&'a str>,
        request: &'a AiRequest,
    ) -> ProviderFuture<'a, AiResponse>;

    fn test<'a>(
        &'a self,
        provider: &'a Provider,
        credential: Option<&'a str>,
    ) -> ProviderFuture<'a, ProviderTestResult>;
}

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleClient {
    client: Client,
}

impl OpenAiCompatibleClient {
    pub fn new() -> Result<Self, AiError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .read_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(45))
            .redirect(redirect::Policy::none())
            .build()
            .map_err(|_| AiError::RequestFailed)?;
        Ok(Self { client })
    }
}

impl ProviderClient for OpenAiCompatibleClient {
    fn complete<'a>(
        &'a self,
        provider: &'a Provider,
        model: &'a AiModel,
        credential: Option<&'a str>,
        request: &'a AiRequest,
    ) -> ProviderFuture<'a, AiResponse> {
        Box::pin(async move {
            ensure_supported(provider.kind)?;
            let endpoint = endpoint(provider, "chat/completions")?;
            let mut builder = self.client.post(endpoint).json(&ChatCompletionRequest {
                model: &model.provider_model_id,
                messages: vec![
                    ChatMessage {
                        role: "developer",
                        content: &request.system_prompt,
                    },
                    ChatMessage {
                        role: "user",
                        content: &request.user_message,
                    },
                ],
                max_tokens: request.max_output_tokens,
            });
            if let Some(secret) = credential {
                builder = builder.bearer_auth(secret);
            }
            let response = builder.send().await.map_err(|_| AiError::EndpointUnavailable)?;
            ensure_success(response.status())?;
            let body = bounded_body(response).await?;
            let parsed: ChatCompletionResponse =
                serde_json::from_slice(&body).map_err(|_| AiError::InvalidResponse)?;
            let content = parsed
                .choices
                .first()
                .and_then(|choice| choice.message.content.as_deref())
                .filter(|value| !value.trim().is_empty())
                .ok_or(AiError::InvalidResponse)?;
            Ok(AiResponse {
                content: content.to_owned(),
                provider_model_id: parsed.model,
                usage: parsed.usage.unwrap_or_default().into(),
            })
        })
    }

    fn test<'a>(
        &'a self,
        provider: &'a Provider,
        credential: Option<&'a str>,
    ) -> ProviderFuture<'a, ProviderTestResult> {
        Box::pin(async move {
            ensure_supported(provider.kind)?;
            let endpoint = endpoint(provider, "models")?;
            let mut builder = self.client.get(endpoint);
            if let Some(secret) = credential {
                builder = builder.bearer_auth(secret);
            }
            let response = builder.send().await.map_err(|_| AiError::EndpointUnavailable)?;
            ensure_success(response.status())?;
            Ok(ProviderTestResult {
                reachable: true,
                sanitized_message: "Provider connection succeeded".to_owned(),
            })
        })
    }
}

fn ensure_supported(kind: ProviderKind) -> Result<(), AiError> {
    if matches!(
        kind,
        ProviderKind::OpenAiCompatible
            | ProviderKind::OpenAi
            | ProviderKind::Ollama
            | ProviderKind::LmStudio
    ) {
        Ok(())
    } else {
        Err(AiError::UnsupportedProvider)
    }
}

fn endpoint(provider: &Provider, suffix: &str) -> Result<Url, AiError> {
    let mut base = Url::parse(&provider.base_url).map_err(|_| AiError::InvalidEndpoint)?;
    if base.username() != "" || base.password().is_some() || base.query().is_some() || base.fragment().is_some() {
        return Err(AiError::InvalidEndpoint);
    }
    match base.scheme() {
        "https" => {}
        "http" if provider.privacy == PrivacyClass::Local && is_loopback(&base) => {}
        _ => return Err(AiError::InvalidEndpoint),
    }
    if !base.path().ends_with('/') {
        let path = format!("{}/", base.path());
        base.set_path(&path);
    }
    base.join(suffix).map_err(|_| AiError::InvalidEndpoint)
}

fn is_loopback(url: &Url) -> bool {
    matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"))
}

fn ensure_success(status: StatusCode) -> Result<(), AiError> {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(AiError::AuthenticationFailed),
        StatusCode::TOO_MANY_REQUESTS => Err(AiError::RateLimited),
        status if status.is_success() => Ok(()),
        _ => Err(AiError::RequestFailed),
    }
}

async fn bounded_body(mut response: reqwest::Response) -> Result<Vec<u8>, AiError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_PROVIDER_RESPONSE_BYTES as u64)
    {
        return Err(AiError::ResponseTooLarge);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| AiError::RequestFailed)? {
        if body.len().saturating_add(chunk.len()) > MAX_PROVIDER_RESPONSE_BYTES {
            return Err(AiError::ResponseTooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    max_tokens: u32,
}

#[derive(Debug, Serialize)]
struct ChatMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    model: String,
    choices: Vec<ChatChoice>,
    usage: Option<ChatUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    content: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ChatUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
    #[serde(default)]
    prompt_tokens_details: PromptTokenDetails,
    #[serde(default)]
    completion_tokens_details: CompletionTokenDetails,
}

#[derive(Debug, Default, Deserialize)]
struct PromptTokenDetails {
    #[serde(default)]
    cached_tokens: u32,
}

#[derive(Debug, Default, Deserialize)]
struct CompletionTokenDetails {
    #[serde(default)]
    reasoning_tokens: u32,
}

impl From<ChatUsage> for AiUsage {
    fn from(value: ChatUsage) -> Self {
        Self {
            input_tokens: value.prompt_tokens,
            output_tokens: value.completion_tokens,
            cached_tokens: value.prompt_tokens_details.cached_tokens,
            reasoning_tokens: value.completion_tokens_details.reasoning_tokens,
        }
    }
}

#[cfg(test)]
mod tests {
    use halquen_domain::{ProviderId, ProviderStatus};

    use super::*;

    fn provider(base_url: &str, privacy: PrivacyClass) -> Provider {
        Provider {
            id: ProviderId::generate(),
            kind: ProviderKind::OpenAiCompatible,
            name: "test".to_owned(),
            base_url: base_url.to_owned(),
            enabled: true,
            privacy,
            configured: true,
            credential_id: None,
            status: ProviderStatus::Configured,
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    #[test]
    fn local_http_is_limited_to_loopback_and_cloud_requires_tls() {
        assert!(endpoint(&provider("http://127.0.0.1:11434/v1", PrivacyClass::Local), "models").is_ok());
        assert!(endpoint(&provider("http://example.com/v1", PrivacyClass::Local), "models").is_err());
        assert!(endpoint(&provider("http://example.com/v1", PrivacyClass::Cloud), "models").is_err());
        assert!(endpoint(&provider("https://example.com/v1", PrivacyClass::Cloud), "models").is_ok());
    }

    #[test]
    fn credentials_embedded_in_urls_are_rejected() {
        assert!(endpoint(&provider("https://user:secret@example.com/v1", PrivacyClass::Cloud), "models").is_err());
    }
}
