use halquen_audit::ExecutionReceipt;
use halquen_domain::{
    ActivityEvent, AgentConfiguration, AgentSession, AiModel, ApplicationSettings,
    CapabilityDescriptor, ChatMessage, ChatSession, PermissionGrant, Provider,
    RegisteredApplication, ResourceLabel, SecurityProfile, UsageStats,
};
use halquen_policy::PolicyDecision;
use serde::{Deserialize, Serialize};

use halquen_memory::{MemoryRevisionView, MemoryView};

use crate::{
    AgentRunResult, ChatResult, ConfirmationResult, DiagnosticsSnapshot, MemoryMutationReceipt,
    PromptPreview, ProviderTestStatus, SecurityOverview,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseEnvelope {
    pub version: u16,
    pub request_id: String,
    pub response: ProtocolResponse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
// Keeping wire DTOs inline preserves a simple, stable public protocol API. Boxing the largest
// payload solely for process-local enum size would leak that implementation detail to every client.
#[allow(clippy::large_enum_variant)]
pub enum ProtocolResponse {
    Health {
        status: HealthStatus,
        schema_version: i64,
    },
    Capabilities {
        capabilities: Vec<CapabilityDescriptor>,
    },
    Capability {
        capability: Option<CapabilityDescriptor>,
    },
    Evaluation {
        decision: PolicyDecision,
    },
    DryRun {
        decision: PolicyDecision,
        receipt: ExecutionReceipt,
    },
    Execution {
        decision: PolicyDecision,
        receipt: ExecutionReceipt,
    },
    MemoryStats {
        items: u64,
        revisions: u64,
        evidence: u64,
        unknown_cases: u64,
    },
    AuditStats {
        records: u64,
        executions: u64,
    },
    Chat {
        result: ChatResult,
    },
    ChatCancellation {
        requested: bool,
    },
    ChatSessions {
        sessions: Vec<ChatSession>,
    },
    ChatMessages {
        messages: Vec<ChatMessage>,
    },
    Activity {
        events: Vec<ActivityEvent>,
    },
    MemoryItems {
        items: Vec<MemoryView>,
    },
    MemoryHistory {
        revisions: Vec<MemoryRevisionView>,
    },
    MemoryUpdated {
        updated: bool,
    },
    MemoryMutation {
        receipt: MemoryMutationReceipt,
    },
    Providers {
        providers: Vec<Provider>,
    },
    ProviderSaved {
        provider: Provider,
    },
    ProviderRemoved {
        removed: bool,
    },
    ProviderTest {
        result: ProviderTestStatus,
    },
    Models {
        models: Vec<AiModel>,
    },
    ModelSaved {
        model: AiModel,
    },
    ApplicationSettings {
        settings: ApplicationSettings,
    },
    SettingsUpdated {
        settings: ApplicationSettings,
    },
    UsageStats {
        stats: UsageStats,
    },
    Diagnostics {
        snapshot: DiagnosticsSnapshot,
    },
    OperationalLogsCleared {
        removed: u64,
    },
    FeedbackRecorded,
    Confirmation {
        result: ConfirmationResult,
    },
    AiRequestPreview {
        preview: PromptPreview,
    },
    SecurityOverview {
        overview: SecurityOverview,
    },
    SecurityProfileUpdated {
        profile: SecurityProfile,
    },
    PermissionGrants {
        grants: Vec<PermissionGrant>,
    },
    PermissionSaved {
        grant: PermissionGrant,
    },
    PermissionRevoked {
        revoked: bool,
    },
    ResourceLabels {
        labels: Vec<ResourceLabel>,
    },
    ResourceLabelSaved {
        label: ResourceLabel,
    },
    ResourceLabelRemoved {
        removed: bool,
    },
    Agents {
        agents: Vec<AgentConfiguration>,
    },
    AgentSaved {
        agent: AgentConfiguration,
    },
    AgentRemoved {
        removed: bool,
    },
    AgentRun {
        result: AgentRunResult,
    },
    AgentSessions {
        sessions: Vec<AgentSession>,
    },
    RegisteredApplications {
        applications: Vec<RegisteredApplication>,
    },
    RegisteredApplicationSaved {
        application: RegisteredApplication,
    },
    RegisteredApplicationRemoved {
        removed: bool,
    },
    Error {
        error: ProtocolErrorBody,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Ok,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolErrorBody {
    pub code: ProtocolErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolErrorCode {
    UnknownVersion,
    MalformedRequest,
    FrameTooLarge,
    InvalidAction,
    NotFound,
    Unsupported,
    Internal,
    Validation,
    PrivacyDenied,
    ProviderUnavailable,
    SecretStoreUnavailable,
    ConfirmationExpired,
    SandboxUnavailable,
}
