use serde::{Deserialize, Serialize};

use halquen_domain::{
    ActionRequest, CapabilityDescriptor, CapabilityId, ExecutionId, ScopeRequirement,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyOutcome {
    Allow,
    Confirm,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum PolicyReason {
    ReadOnlyBaseline,
    LocalSideEffectBaseline,
    ReversibleLocalWriteBaseline,
    ReversibleLocalWriteRequiresConfirmation,
    ExternalSideEffectRequiresConfirmation,
    DestructiveRequiresConfirmation,
    PrivilegedDenied,
    UnknownRiskDenied,
    CapabilityRequiresConfirmation,
    MissingScope { scope: ScopeRequirement },
    InvalidDescriptor,
    InvalidActionContract,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyDecision {
    pub outcome: PolicyOutcome,
    pub reason: PolicyReason,
}

impl PolicyDecision {
    pub fn is_allowed(&self) -> bool {
        self.outcome == PolicyOutcome::Allow
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ExecutionAuthorization {
    execution_id: ExecutionId,
    descriptor: CapabilityDescriptor,
    action: ActionRequest,
    granted_scopes: Vec<ScopeRequirement>,
}

impl ExecutionAuthorization {
    pub(crate) fn new(
        execution_id: ExecutionId,
        descriptor: CapabilityDescriptor,
        action: ActionRequest,
        granted_scopes: Vec<ScopeRequirement>,
    ) -> Self {
        Self {
            execution_id,
            descriptor,
            action,
            granted_scopes,
        }
    }

    pub fn execution_id(&self) -> &ExecutionId {
        &self.execution_id
    }

    pub fn capability_id(&self) -> &CapabilityId {
        &self.descriptor.id
    }

    pub fn capability_version(&self) -> u32 {
        self.descriptor.version
    }

    pub fn descriptor(&self) -> &CapabilityDescriptor {
        &self.descriptor
    }

    pub fn action(&self) -> &ActionRequest {
        &self.action
    }

    pub fn granted_scopes(&self) -> &[ScopeRequirement] {
        &self.granted_scopes
    }

    pub fn matches(&self, descriptor: &CapabilityDescriptor, action: &ActionRequest) -> bool {
        &self.descriptor == descriptor && &self.action == action
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PolicyEvaluation {
    pub decision: PolicyDecision,
    authorization: Option<ExecutionAuthorization>,
}

impl PolicyEvaluation {
    pub(crate) fn allowed(decision: PolicyDecision, authorization: ExecutionAuthorization) -> Self {
        Self {
            decision,
            authorization: Some(authorization),
        }
    }

    pub(crate) fn blocked(decision: PolicyDecision) -> Self {
        Self {
            decision,
            authorization: None,
        }
    }

    pub fn authorization(&self) -> Option<&ExecutionAuthorization> {
        self.authorization.as_ref()
    }

    pub fn into_authorization(self) -> Option<ExecutionAuthorization> {
        self.authorization
    }
}
