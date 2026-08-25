use halquen_domain::{CapabilityId, EventId, ExecutionId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpisodicRecord {
    pub event_id: EventId,
    pub execution_id: Option<ExecutionId>,
    pub capability_id: Option<CapabilityId>,
    pub outcome_code: String,
    pub created_at_ms: i64,
}
