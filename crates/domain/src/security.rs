use serde::{Deserialize, Serialize};

use crate::{
    ActionArguments, ActionRequest, AgentId, AgentInstanceId, AgentSessionId, BehaviourEventId,
    CapabilityId, ChatSessionId, DaemonSessionId, EntityId, ExecutableIdentity,
    ExecutableOwnership, PermissionId, ResourceLabelId, TrustClass,
};

pub const MAX_PROVENANCE_HOPS: usize = 8;
pub const MAX_ACTION_RESOURCES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionOrigin {
    UserExplicit,
    LocalResolver,
    AiProposal,
    ExternalContent,
    Plugin,
    StoredProcedure,
    Agent,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityClass {
    None,
    User,
    HalquenCore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    User,
    Halquen,
    Model,
    Plugin,
    StoredProcedure,
    Agent,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceHop {
    pub origin: ActionOrigin,
    pub actor: ActorKind,
    pub actor_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionProvenance {
    pub initiating_request_id: Option<String>,
    pub hops: Vec<ProvenanceHop>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceClassification {
    Public,
    Local,
    Personal,
    Sensitive,
    Secret,
    Production,
    SystemCritical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataClassification {
    Public,
    Personal,
    Sensitive,
    Secret,
    Production,
}

impl DataClassification {
    pub fn inherited_from(
        sources: impl IntoIterator<Item = DataClassification>,
    ) -> DataClassification {
        sources
            .into_iter()
            .max()
            .unwrap_or(DataClassification::Public)
    }

    pub fn declassify(
        self,
        target: DataClassification,
        authority: TrustedDeclassificationAuthority,
    ) -> DataClassification {
        let _trusted_authority = authority;
        target.min(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustedDeclassificationAuthority {
    UserExplicit,
    HalquenPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Application,
    File,
    NetworkEndpoint,
    Database,
    Agent,
    Plugin,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceDescriptor {
    pub kind: ResourceKind,
    pub identifier: String,
    pub classification: ResourceClassification,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DestinationClass {
    Local,
    TrustedEndpoint,
    External,
    UntrustedExternal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataFlowContext {
    pub source: ResourceDescriptor,
    pub data_classification: DataClassification,
    pub destination: DestinationClass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionContext {
    pub origin: ActionOrigin,
    pub trust: TrustClass,
    pub authority: AuthorityClass,
    pub provenance: ActionProvenance,
    pub resources: Vec<ResourceDescriptor>,
    pub data_flow: Option<DataFlowContext>,
    pub agent: Option<AgentExecutionIdentity>,
}

impl ActionContext {
    pub fn trusted_user(request_id: Option<String>) -> Self {
        Self {
            origin: ActionOrigin::UserExplicit,
            trust: TrustClass::UserExplicit,
            authority: AuthorityClass::User,
            provenance: ActionProvenance {
                initiating_request_id: request_id,
                hops: vec![ProvenanceHop {
                    origin: ActionOrigin::UserExplicit,
                    actor: ActorKind::User,
                    actor_id: None,
                }],
            },
            resources: Vec::new(),
            data_flow: None,
            agent: None,
        }
    }

    pub fn local_resolution(request_id: Option<String>) -> Self {
        let mut context = Self::trusted_user(request_id);
        context.origin = ActionOrigin::LocalResolver;
        context.authority = AuthorityClass::HalquenCore;
        context.provenance.hops.push(ProvenanceHop {
            origin: ActionOrigin::LocalResolver,
            actor: ActorKind::Halquen,
            actor_id: Some("local-resolver".to_owned()),
        });
        context
    }

    pub fn untrusted(origin: ActionOrigin, actor_id: Option<String>) -> Option<Self> {
        let (trust, actor) = match origin {
            ActionOrigin::AiProposal => (TrustClass::AiInferred, ActorKind::Model),
            ActionOrigin::ExternalContent => (TrustClass::ExternalContent, ActorKind::System),
            ActionOrigin::Plugin => (TrustClass::PluginAsserted, ActorKind::Plugin),
            ActionOrigin::Agent => (TrustClass::ExternalContent, ActorKind::Agent),
            ActionOrigin::UserExplicit
            | ActionOrigin::LocalResolver
            | ActionOrigin::StoredProcedure
            | ActionOrigin::System => return None,
        };
        Some(Self {
            origin,
            trust,
            authority: AuthorityClass::None,
            provenance: ActionProvenance {
                initiating_request_id: None,
                hops: vec![ProvenanceHop {
                    origin,
                    actor,
                    actor_id,
                }],
            },
            resources: Vec::new(),
            data_flow: None,
            agent: None,
        })
    }

    pub fn agent(identity: AgentExecutionIdentity) -> Self {
        Self {
            origin: ActionOrigin::Agent,
            trust: TrustClass::ExternalContent,
            authority: AuthorityClass::None,
            provenance: ActionProvenance {
                initiating_request_id: None,
                hops: vec![ProvenanceHop {
                    origin: ActionOrigin::Agent,
                    actor: ActorKind::Agent,
                    actor_id: Some(identity.agent_id.to_string()),
                }],
            },
            resources: Vec::new(),
            data_flow: None,
            agent: Some(identity),
        }
    }

    pub fn with_resource(mut self, resource: ResourceDescriptor) -> Self {
        if self.resources.len() < MAX_ACTION_RESOURCES {
            self.resources.push(resource);
        }
        self
    }

    pub fn with_data_flow(mut self, data_flow: DataFlowContext) -> Self {
        self.data_flow = Some(data_flow);
        self
    }

    pub fn validate(&self) -> Result<(), SecurityValidationError> {
        if self.provenance.hops.is_empty()
            || self.provenance.hops.len() > MAX_PROVENANCE_HOPS
            || self.resources.len() > MAX_ACTION_RESOURCES
        {
            return Err(SecurityValidationError::InvalidBounds);
        }
        if self
            .provenance
            .initiating_request_id
            .as_ref()
            .is_some_and(|value| !valid_identifier(value, 128))
            || self.provenance.hops.iter().any(|hop| {
                hop.actor_id
                    .as_ref()
                    .is_some_and(|value| !valid_identifier(value, 128))
            })
            || self.resources.iter().any(|resource| {
                resource.identifier.trim().is_empty() || resource.identifier.len() > 1_024
            })
        {
            return Err(SecurityValidationError::InvalidBounds);
        }
        if self.provenance.hops.last().map(|hop| hop.origin) != Some(self.origin) {
            return Err(SecurityValidationError::InvalidProvenance);
        }
        match self.origin {
            ActionOrigin::UserExplicit
                if self.trust != TrustClass::UserExplicit
                    || self.authority != AuthorityClass::User =>
            {
                Err(SecurityValidationError::InvalidAuthority)
            }
            ActionOrigin::LocalResolver
                if self.authority != AuthorityClass::HalquenCore
                    || self.provenance.hops.first().map(|hop| hop.origin)
                        != Some(ActionOrigin::UserExplicit) =>
            {
                Err(SecurityValidationError::InvalidAuthority)
            }
            ActionOrigin::AiProposal | ActionOrigin::ExternalContent | ActionOrigin::Plugin
                if self.authority != AuthorityClass::None =>
            {
                Err(SecurityValidationError::InvalidAuthority)
            }
            ActionOrigin::Agent
                if self.authority != AuthorityClass::None || self.agent.is_none() =>
            {
                Err(SecurityValidationError::InvalidAuthority)
            }
            _ if self.origin != ActionOrigin::Agent && self.agent.is_some() => {
                Err(SecurityValidationError::InvalidProvenance)
            }
            _ => Ok(()),
        }
    }

    pub fn sanitized_summary(&self) -> ActionContextSummary {
        ActionContextSummary {
            origin: self.origin,
            trust: self.trust,
            authority: self.authority,
            provenance: self.provenance.hops.iter().map(|hop| hop.origin).collect(),
            resource_kinds: self.resources.iter().map(|item| item.kind).collect(),
            resource_classifications: self
                .resources
                .iter()
                .map(|item| item.classification)
                .collect(),
            data_classification: self.data_flow.as_ref().map(|flow| flow.data_classification),
            destination: self.data_flow.as_ref().map(|flow| flow.destination),
            agent: self.agent.clone(),
        }
    }
}

fn valid_identifier(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'.' | b'_' | b'-'))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionContextSummary {
    pub origin: ActionOrigin,
    pub trust: TrustClass,
    pub authority: AuthorityClass,
    pub provenance: Vec<ActionOrigin>,
    pub resource_kinds: Vec<ResourceKind>,
    pub resource_classifications: Vec<ResourceClassification>,
    pub data_classification: Option<DataClassification>,
    pub destination: Option<DestinationClass>,
    pub agent: Option<AgentExecutionIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentExecutionIdentity {
    pub agent_id: AgentId,
    pub instance_id: AgentInstanceId,
    pub session_id: AgentSessionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionProposal {
    pub action: ActionRequest,
    pub context: ActionContext,
}

impl ActionProposal {
    pub fn new(
        action: ActionRequest,
        context: ActionContext,
    ) -> Result<Self, SecurityValidationError> {
        context.validate()?;
        Ok(Self { action, context })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityProfile {
    Strict,
    Balanced,
    Developer,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionEffect {
    Allow,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionLifetime {
    Once,
    Session,
    Until,
    Always,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum PermissionSessionScope {
    Chat(ChatSessionId),
    Agent(AgentSessionId),
    Daemon(DaemonSessionId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionScope {
    pub capability_id: CapabilityId,
    pub arguments: ActionArguments,
    pub resources: Vec<ResourceDescriptor>,
    pub destination: Option<DestinationClass>,
}

impl PermissionScope {
    pub fn from_proposal(proposal: &ActionProposal) -> Self {
        Self {
            capability_id: proposal.action.capability_id.clone(),
            arguments: proposal.action.arguments.clone(),
            resources: proposal.context.resources.clone(),
            destination: proposal
                .context
                .data_flow
                .as_ref()
                .map(|flow| flow.destination),
        }
    }

    pub fn matches(&self, proposal: &ActionProposal) -> bool {
        self.capability_id == proposal.action.capability_id
            && self.arguments == proposal.action.arguments
            && self.resources == proposal.context.resources
            && self.destination
                == proposal
                    .context
                    .data_flow
                    .as_ref()
                    .map(|flow| flow.destination)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionGrant {
    pub id: PermissionId,
    pub effect: PermissionEffect,
    pub lifetime: PermissionLifetime,
    pub scope: PermissionScope,
    pub session: Option<PermissionSessionScope>,
    pub agent_id: Option<AgentId>,
    pub granted_by: ActionOrigin,
    pub granted_at_ms: i64,
    pub expires_at_ms: Option<i64>,
    pub revoked_at_ms: Option<i64>,
    pub use_limit: Option<u32>,
    pub use_count: u32,
}

impl PermissionGrant {
    pub fn is_active(
        &self,
        now_ms: i64,
        session: Option<&PermissionSessionScope>,
        agent_id: Option<&AgentId>,
    ) -> bool {
        self.revoked_at_ms.is_none()
            && self.expires_at_ms.is_none_or(|expiry| expiry >= now_ms)
            && self.use_limit.is_none_or(|limit| self.use_count < limit)
            && match self.lifetime {
                PermissionLifetime::Session => self.session.as_ref() == session,
                _ => true,
            }
            && match agent_id {
                Some(agent_id) => self.agent_id.as_ref() == Some(agent_id),
                None => self.agent_id.is_none(),
            }
    }

    pub fn validate(&self) -> Result<(), SecurityValidationError> {
        if !matches!(
            self.granted_by,
            ActionOrigin::UserExplicit | ActionOrigin::System
        ) || self.scope.resources.len() > MAX_ACTION_RESOURCES
            || self
                .expires_at_ms
                .is_some_and(|expiry| expiry < self.granted_at_ms)
            || matches!(self.lifetime, PermissionLifetime::Once) && self.use_limit != Some(1)
            || matches!(self.lifetime, PermissionLifetime::Session) && self.session.is_none()
            || !matches!(self.lifetime, PermissionLifetime::Session) && self.session.is_some()
            || matches!(self.session, Some(PermissionSessionScope::Agent(_)))
                && self.agent_id.is_none()
            || matches!(
                self.session,
                Some(PermissionSessionScope::Chat(_) | PermissionSessionScope::Daemon(_))
            ) && self.agent_id.is_some()
            || matches!(self.lifetime, PermissionLifetime::Until) && self.expires_at_ms.is_none()
        {
            return Err(SecurityValidationError::InvalidPermission);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceMatchKind {
    Exact,
    PathPrefix,
    Host,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLabel {
    pub id: ResourceLabelId,
    pub name: String,
    pub resource_kind: ResourceKind,
    pub match_kind: ResourceMatchKind,
    pub pattern: String,
    pub classification: ResourceClassification,
    pub data_classification: DataClassification,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl ResourceLabel {
    pub fn validate(&self) -> Result<(), SecurityValidationError> {
        if self.name.trim().is_empty()
            || self.name.len() > 128
            || self.pattern.trim().is_empty()
            || self.pattern.len() > 1_024
            || self.updated_at_ms < self.created_at_ms
        {
            return Err(SecurityValidationError::InvalidResourceLabel);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BehaviourOutcome {
    Success,
    Failure,
    CorrectionAccepted,
    CorrectionRejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentUsageEvent {
    pub id: BehaviourEventId,
    pub intent: String,
    pub entity_id: EntityId,
    pub outcome: BehaviourOutcome,
    pub context_class: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentCandidate {
    pub entity_id: EntityId,
    pub score_permille: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTransport {
    Cli,
    UnixSocket,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxBackend {
    Bubblewrap,
    Unavailable,
    UnsafeUnsandboxed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentResourceLimits {
    pub cpu_seconds: u32,
    pub memory_bytes: u64,
    pub process_count: u32,
    pub file_size_bytes: u64,
    pub open_files: u32,
    pub temp_bytes: u64,
}

impl Default for AgentResourceLimits {
    fn default() -> Self {
        Self {
            cpu_seconds: 30,
            memory_bytes: 512 * 1024 * 1024,
            process_count: 64,
            file_size_bytes: 16 * 1024 * 1024,
            open_files: 128,
            temp_bytes: 64 * 1024 * 1024,
        }
    }
}

impl AgentResourceLimits {
    pub fn validate(&self) -> bool {
        (1..=300).contains(&self.cpu_seconds)
            && (16 * 1024 * 1024..=8 * 1024 * 1024 * 1024).contains(&self.memory_bytes)
            && (1..=1_024).contains(&self.process_count)
            && (1_024..=1024 * 1024 * 1024).contains(&self.file_size_bytes)
            && (16..=4_096).contains(&self.open_files)
            && (1024 * 1024..=1024 * 1024 * 1024).contains(&self.temp_bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentConfiguration {
    pub id: AgentId,
    pub name: String,
    pub transport: AgentTransport,
    pub executable: String,
    pub arguments: Vec<String>,
    pub socket_path: Option<String>,
    pub sandbox: SandboxBackend,
    pub ownership: ExecutableOwnership,
    pub executable_identity: Option<ExecutableIdentity>,
    pub resource_limits: AgentResourceLimits,
    pub enabled: bool,
    pub timeout_ms: u64,
    pub max_stdout_bytes: u32,
    pub max_stderr_bytes: u32,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionStatus {
    Running,
    Completed,
    Failed,
    TimedOut,
    Crashed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: AgentSessionId,
    pub agent_id: AgentId,
    pub instance_id: AgentInstanceId,
    pub daemon_session_id: DaemonSessionId,
    pub status: AgentSessionStatus,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonSession {
    pub id: DaemonSessionId,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
}

impl AgentConfiguration {
    pub fn validate(&self) -> Result<(), SecurityValidationError> {
        let cli_valid = self.transport != AgentTransport::Cli
            || (self.executable.starts_with('/')
                && self.executable.len() <= 1_024
                && self.arguments.len() <= 32
                && self
                    .arguments
                    .iter()
                    .all(|argument| argument.len() <= 1_024));
        let socket_valid = self.transport != AgentTransport::UnixSocket
            || self
                .socket_path
                .as_ref()
                .is_some_and(|path| path.starts_with('/') && path.len() <= 1_024);
        if self.name.trim().is_empty()
            || self.name.len() > 128
            || !cli_valid
            || !socket_valid
            || !self.resource_limits.validate()
            || self
                .executable_identity
                .as_ref()
                .is_some_and(|identity| identity.validate().is_err())
            || !(100..=300_000).contains(&self.timeout_ms)
            || !(1_024..=1_048_576).contains(&self.max_stdout_bytes)
            || !(1_024..=1_048_576).contains(&self.max_stderr_bytes)
            || self.updated_at_ms < self.created_at_ms
        {
            return Err(SecurityValidationError::InvalidAgentConfiguration);
        }
        Ok(())
    }

    pub fn sandbox_is_enforced(&self) -> bool {
        self.sandbox == SandboxBackend::Bubblewrap
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityValidationError {
    InvalidBounds,
    InvalidProvenance,
    InvalidAuthority,
    InvalidPermission,
    InvalidResourceLabel,
    InvalidAgentConfiguration,
    InvalidExecutableIdentity,
    InvalidApplicationRegistration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_sources_cannot_construct_user_explicit_context() {
        assert!(ActionContext::untrusted(ActionOrigin::UserExplicit, None).is_none());
        let ai = ActionContext::untrusted(ActionOrigin::AiProposal, Some("model".to_owned()))
            .expect("AI is a supported untrusted origin");
        assert_eq!(ai.authority, AuthorityClass::None);
        assert_eq!(ai.trust, TrustClass::AiInferred);
        assert!(ai.validate().is_ok());
    }

    #[test]
    fn local_resolution_keeps_the_user_to_resolver_chain() {
        let context = ActionContext::local_resolution(Some("request:test".to_owned()));
        assert_eq!(
            context.sanitized_summary().provenance,
            vec![ActionOrigin::UserExplicit, ActionOrigin::LocalResolver]
        );
        assert!(context.validate().is_ok());
    }

    #[test]
    fn spoofed_user_origin_without_user_authority_is_rejected() {
        let mut context = ActionContext::trusted_user(None);
        context.authority = AuthorityClass::None;
        assert_eq!(
            context.validate(),
            Err(SecurityValidationError::InvalidAuthority)
        );
    }

    #[test]
    fn derived_data_inherits_the_most_sensitive_source() {
        let permutations = [
            [
                DataClassification::Public,
                DataClassification::Secret,
                DataClassification::Personal,
            ],
            [
                DataClassification::Public,
                DataClassification::Personal,
                DataClassification::Secret,
            ],
            [
                DataClassification::Secret,
                DataClassification::Public,
                DataClassification::Personal,
            ],
            [
                DataClassification::Secret,
                DataClassification::Personal,
                DataClassification::Public,
            ],
            [
                DataClassification::Personal,
                DataClassification::Public,
                DataClassification::Secret,
            ],
            [
                DataClassification::Personal,
                DataClassification::Secret,
                DataClassification::Public,
            ],
        ];
        for sources in permutations {
            assert_eq!(
                DataClassification::inherited_from(sources),
                DataClassification::Secret
            );
        }
    }

    #[test]
    fn declassification_requires_a_trusted_typed_authority() {
        assert_eq!(
            DataClassification::Secret.declassify(
                DataClassification::Personal,
                TrustedDeclassificationAuthority::UserExplicit,
            ),
            DataClassification::Personal
        );
    }
}
