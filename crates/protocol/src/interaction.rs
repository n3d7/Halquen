use halquen_domain::{
    ActionArgumentKind, ActionArguments, AgentConfiguration, AgentId, AgentResourceLimits,
    AgentSession, AgentSessionId, AgentTransport, AiModel, AiTaskType, ApplicationSettings,
    CapabilityId, ChatMessage, ChatSession, ContextCategory, DataClassification, DestinationClass,
    EntityId, ExecutableOwnership, ExecutionId, MemoryId, MemoryRevisionId, ModelId,
    ModelSelection, PermissionEffect, PermissionId, PermissionLifetime, PermissionSessionScope,
    PrivacyClass, ProviderId, ProviderKind, ProviderStatus, RegisteredApplication,
    ResourceClassification, ResourceDescriptor, ResourceKind, ResourceLabelId, ResourceMatchKind,
    RiskClass, SandboxBackend, SecurityProfile,
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
    pub operation: String,
    pub target: String,
    pub destination: Option<DestinationClass>,
    pub origin: halquen_domain::ActionOrigin,
    pub resource_classifications: Vec<ResourceClassification>,
    pub agent_id: Option<AgentId>,
    pub agent_session_id: Option<AgentSessionId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmationPersistence {
    Once,
    Session,
    Until,
    Always,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionGrantUpsert {
    pub id: Option<PermissionId>,
    pub effect: PermissionEffect,
    pub lifetime: PermissionLifetime,
    pub capability_id: halquen_domain::CapabilityId,
    pub arguments: ActionArguments,
    pub resources: Vec<ResourceDescriptor>,
    pub destination: Option<DestinationClass>,
    pub session: Option<PermissionSessionScope>,
    pub agent_id: Option<AgentId>,
    pub expires_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLabelUpsert {
    pub id: Option<ResourceLabelId>,
    pub name: String,
    pub resource_kind: ResourceKind,
    pub match_kind: ResourceMatchKind,
    pub pattern: String,
    pub classification: ResourceClassification,
    pub data_classification: DataClassification,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentConfigurationUpsert {
    pub id: Option<AgentId>,
    pub name: String,
    pub transport: AgentTransport,
    pub executable: String,
    pub arguments: Vec<String>,
    pub socket_path: Option<String>,
    pub sandbox: SandboxBackend,
    pub ownership: ExecutableOwnership,
    pub sha256_hex: Option<String>,
    pub resource_limits: AgentResourceLimits,
    pub enabled: bool,
    pub timeout_ms: u64,
    pub max_stdout_bytes: u32,
    pub max_stderr_bytes: u32,
}

impl AgentConfigurationUpsert {
    pub fn into_configuration(
        self,
        now_ms: i64,
        executable_identity: Option<halquen_domain::ExecutableIdentity>,
    ) -> AgentConfiguration {
        AgentConfiguration {
            id: self.id.unwrap_or_else(AgentId::generate),
            name: self.name,
            transport: self.transport,
            executable: self.executable,
            arguments: self.arguments,
            socket_path: self.socket_path,
            sandbox: self.sandbox,
            ownership: self.ownership,
            executable_identity,
            resource_limits: self.resource_limits,
            enabled: self.enabled,
            timeout_ms: self.timeout_ms,
            max_stdout_bytes: self.max_stdout_bytes,
            max_stderr_bytes: self.max_stderr_bytes,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityOverview {
    pub profile: SecurityProfile,
    pub immutable_rule_ids: Vec<String>,
    pub active_permissions: u32,
    pub resource_labels: u32,
    pub configured_agents: u32,
    pub active_agent_sessions: u32,
    pub registered_applications: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationRegistrationUpsert {
    pub entity_id: EntityId,
    pub display_name: String,
    pub executable: String,
    pub arguments: Vec<String>,
    pub ownership: ExecutableOwnership,
    pub sha256_hex: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunRequest {
    pub agent_id: AgentId,
    pub input: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityDiscovery {
    pub id: CapabilityId,
    pub version: u32,
    pub description: String,
    pub risk: RiskClass,
    pub arguments: ActionArgumentKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentProposalDisposition {
    Executed,
    Simulated,
    ConfirmationRequired,
    Denied,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProposalResult {
    pub index: u16,
    pub capability_id: CapabilityId,
    pub disposition: AgentProposalDisposition,
    pub execution_id: Option<ExecutionId>,
    pub confirmation: Option<ConfirmationPrompt>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunResult {
    pub session: AgentSession,
    pub message: String,
    pub proposals: Vec<AgentProposalResult>,
    pub stderr_bytes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationRegistrationResult {
    pub application: RegisteredApplication,
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
