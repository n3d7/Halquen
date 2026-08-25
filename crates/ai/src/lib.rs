#![forbid(unsafe_code)]

mod context;
mod prompt;
mod provider;
mod router;
mod secret;
mod types;

pub use context::{ContextBuilder, ContextItem, ContextProjection};
pub use prompt::{CORE_SECURITY_CONTRACT, PromptComposer, PromptProfile};
pub use provider::{OpenAiCompatibleClient, ProviderClient, ProviderFuture};
pub use router::{ModelRouter, RouteError, RouteRequest, SelectedModel};
pub use secret::{KeyringSecretStore, SecretError, SecretStore};
pub use types::{AiError, AiRequest, AiResponse, AiUsage, ProviderTestResult};
