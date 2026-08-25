#![forbid(unsafe_code)]

pub mod action;
pub mod capability;
pub mod entity;
pub mod event;
mod ids;
pub mod intent;
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
    AuditId, CapabilityId, CorrectionId, EntityId, EventId, EvidenceId, ExecutionId, IdError,
    IntentId, MemoryId, MemoryRevisionId, ProposalId, UnknownCaseId,
};
pub use intent::{AiProposal, Intent, ProposalPayload, ProposalStatus};
pub use trust::TrustClass;
