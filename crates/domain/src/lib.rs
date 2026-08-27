#![forbid(unsafe_code)]

pub mod action;
pub mod application;
pub mod capability;
pub mod entity;
pub mod event;
mod ids;
pub mod intent;
pub mod interaction;
pub mod security;
pub mod trust;

pub use action::{ActionArgumentKind, ActionArguments, ActionRequest};
pub use application::{ExecutableIdentity, ExecutableOwnership, RegisteredApplication};

pub use capability::{
    CapabilityDescriptor, CapabilityValidationError, ConfirmationPolicy, RiskClass,
    ScopeRequirement,
};

pub use entity::{Entity, EntityKind};
pub use event::{Correction, Event, EventKind, QueueStatus, UnknownCase};
pub use ids::{
    ActivityId, AgentId, AgentInstanceId, AgentSessionId, AuditId, BehaviourEventId, CacheEntryId,
    CapabilityId, ChatMessageId, ChatSessionId, CorrectionId, DaemonSessionId, EntityId, EventId,
    EvidenceId, ExecutionId, IdError, IntentId, MemoryId, MemoryRevisionId, ModelId, PermissionId,
    ProposalId, ProviderId, ResourceLabelId, UnknownCaseId,
};
pub use intent::{AiProposal, Intent, ProposalPayload, ProposalStatus};
pub use interaction::{
    ActivityEvent, ActivityKind, AiModel, AiTaskType, AppearanceMode, ApplicationSettings,
    CachedResponse, ChatMessage, ChatOrigin, ChatRole, ChatRoute, ChatSession, ContextCategory,
    DiagnosticEntry, DiagnosticSeverity, LogLevel, ModelSelection, PrivacyClass, Provider,
    ProviderKind, ProviderStatus, ResponseFeedback, RoutingPreset, UsageStats,
};
pub use security::{
    ActionContext, ActionContextSummary, ActionOrigin, ActionProposal, ActionProvenance, ActorKind,
    AgentConfiguration, AgentExecutionIdentity, AgentResourceLimits, AgentSession,
    AgentSessionStatus, AgentTransport, AuthorityClass, BehaviourOutcome, DaemonSession,
    DataClassification, DataFlowContext, DestinationClass, IntentCandidate, IntentUsageEvent,
    PermissionEffect, PermissionGrant, PermissionLifetime, PermissionScope, PermissionSessionScope,
    ProvenanceHop, ResourceClassification, ResourceDescriptor, ResourceKind, ResourceLabel,
    ResourceMatchKind, SandboxBackend, SecurityProfile, SecurityValidationError,
    TrustedDeclassificationAuthority,
};
pub use trust::TrustClass;
