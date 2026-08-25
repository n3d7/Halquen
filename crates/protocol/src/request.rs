use halquen_domain::{ActionRequest, CapabilityId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestEnvelope {
    pub version: u16,
    pub request_id: String,
    pub request: ProtocolRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum ProtocolRequest {
    Health,
    ListCapabilities,
    GetCapability { capability_id: CapabilityId },
    EvaluateAction { action: ActionRequest },
    DryRunAction { action: ActionRequest },
    MemoryStats,
    AuditStats,
}
