use halquen_audit::ExecutionReceipt;
use halquen_domain::CapabilityDescriptor;
use halquen_policy::PolicyDecision;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseEnvelope {
    pub version: u16,
    pub request_id: String,
    pub response: ProtocolResponse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProtocolResponse {
    Health { status: HealthStatus, schema_version: i64 },
    Capabilities { capabilities: Vec<CapabilityDescriptor> },
    Capability { capability: Option<CapabilityDescriptor> },
    Evaluation { decision: PolicyDecision },
    DryRun {
        decision: PolicyDecision,
        receipt: ExecutionReceipt,
    },
    MemoryStats {
        items: u64,
        revisions: u64,
        evidence: u64,
        unknown_cases: u64,
    },
    AuditStats { records: u64, executions: u64 },
    Error { error: ProtocolErrorBody },
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
}
