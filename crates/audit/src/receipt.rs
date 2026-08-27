use halquen_domain::{
    ActionContextSummary, AgentExecutionIdentity, AgentSessionId, AgentSessionStatus, CapabilityId,
    ExecutionId, MemoryId, MemoryRevisionId, ProposalId,
};
use halquen_policy::PolicyDecision;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    DryRunSucceeded,
    Succeeded,
    Failed,
    TimedOut,
    NotExecuted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafeResultCode {
    Simulated,
    Launched,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    pub execution_id: ExecutionId,
    pub capability_id: CapabilityId,
    pub capability_version: u32,
    pub started_at_ms: i64,
    pub finished_at_ms: i64,
    pub policy_decision: PolicyDecision,
    pub status: ExecutionStatus,
    pub reversible: bool,
    pub result_code: Option<SafeResultCode>,
    pub error_code: Option<String>,
    pub sanitized_error: Option<String>,
    pub compensation_reference: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditRecord {
    pub id: halquen_domain::AuditId,
    pub created_at_ms: i64,
    pub event: AuditEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuditEvent {
    ProposalCreated {
        execution_id: ExecutionId,
        proposal_id: ProposalId,
        capability_id: CapabilityId,
        context: ActionContextSummary,
    },
    ActionRequested {
        execution_id: ExecutionId,
        capability_id: CapabilityId,
        capability_version: u32,
    },
    PolicyEvaluated {
        execution_id: Option<ExecutionId>,
        capability_id: CapabilityId,
        decision: PolicyDecision,
    },
    ConfirmationRequired {
        execution_id: ExecutionId,
        capability_id: CapabilityId,
    },
    ConfirmationReceived {
        execution_id: ExecutionId,
        capability_id: CapabilityId,
        accepted: bool,
        agent: Option<AgentExecutionIdentity>,
    },
    AuthorizationCreated {
        execution_id: ExecutionId,
        capability_id: CapabilityId,
        agent: Option<AgentExecutionIdentity>,
    },
    ActionDenied {
        execution_id: ExecutionId,
        capability_id: CapabilityId,
    },
    ExecutionStarted {
        execution_id: ExecutionId,
        capability_id: CapabilityId,
    },
    ExecutionCompleted {
        execution_id: ExecutionId,
        capability_id: CapabilityId,
        result_code: Option<SafeResultCode>,
    },
    ExecutionFailed {
        execution_id: ExecutionId,
        capability_id: CapabilityId,
        error_code: String,
    },
    ExecutionTimedOut {
        execution_id: ExecutionId,
        capability_id: CapabilityId,
        error_code: String,
    },
    AgentSessionStarted {
        session_id: AgentSessionId,
        agent: AgentExecutionIdentity,
    },
    AgentSessionFinished {
        session_id: AgentSessionId,
        status: AgentSessionStatus,
    },
    MemoryRevision {
        memory_id: MemoryId,
        revision_id: MemoryRevisionId,
    },
}
