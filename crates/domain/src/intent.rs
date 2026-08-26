use serde::{Deserialize, Serialize};

use crate::{ActionRequest, CapabilityId, EntityId, EvidenceId, IntentId, MemoryId, ProposalId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Intent {
    pub id: IntentId,
    pub canonical_name: String,
    pub capability_id: CapabilityId,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Pending,
    Accepted,
    Rejected,
    Superseded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProposalPayload {
    Alias {
        entity_id: EntityId,
        alias: String,
    },
    IntentInterpretation {
        intent_id: IntentId,
        capability_id: CapabilityId,
    },
    MemoryUpdate {
        memory_id: MemoryId,
        summary: String,
    },
    Routine {
        name: String,
        steps: Vec<ActionRequest>,
    },
    Plan {
        actions: Vec<ActionRequest>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiProposal {
    pub id: ProposalId,
    pub provider_model: String,
    pub created_at_ms: i64,
    pub payload: ProposalPayload,
    pub status: ProposalStatus,
    pub evidence_ids: Vec<EvidenceId>,
}
