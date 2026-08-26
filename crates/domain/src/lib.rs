#![forbid(unsafe_code)]

pub mod action;
pub mod capability;
pub mod entity;
pub mod event;
mod ids;
pub mod intent;
pub mod interaction;
pub mod trust;

pub use action::{
    ActionArgumentKind,
    ActionArguments,
    ActionRequest,
};

pub use capability::{
    CapabilityDescriptor,
    CapabilityValidationError,
    ConfirmationPolicy,
    RiskClass,
    ScopeRequirement,
};

pub use entity::{Entity, EntityKind};
pub use event::{Correction, Event, EventKind, QueueStatus, UnknownCase};
pub use ids::{
    ActivityId, AuditId, CacheEntryId, CapabilityId, ChatMessageId, ChatSessionId, CorrectionId,
    EntityId, EventId, EvidenceId, ExecutionId, IdError, IntentId, MemoryId, MemoryRevisionId,
    ModelId, ProposalId, ProviderId, UnknownCaseId,
};
pub use intent::{AiProposal, Intent, ProposalPayload, ProposalStatus};
pub use interaction::{
    ActivityEvent, ActivityKind, AiModel, AiTaskType, AppearanceMode, ApplicationSettings,
    CachedResponse, ChatMessage, ChatOrigin, ChatRole, ChatRoute, ChatSession, ContextCategory,
    DiagnosticEntry, DiagnosticSeverity, LogLevel, ModelSelection, PrivacyClass, Provider,
    ProviderKind, ProviderStatus, ResponseFeedback, RoutingPreset, UsageStats,
};
pub use trust::TrustClass;
