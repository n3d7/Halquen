use halquen_domain::{EvidenceId, MemoryRevisionId, TrustClass};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub id: EvidenceId,
    pub trust: TrustClass,
    pub source_reference: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryEvidence {
    pub revision_id: MemoryRevisionId,
    pub evidence_id: EvidenceId,
}
