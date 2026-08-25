use serde::{Deserialize, Serialize};

use crate::{CorrectionId, EventId, UnknownCaseId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub kind: EventKind,
    pub subject_id: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    ActionRequested,
    PolicyEvaluated,
    ConfirmationRequired,
    ActionDenied,
    ExecutionStarted,
    ExecutionCompleted,
    ExecutionFailed,
    ExecutionTimedOut,
    MemoryRevision,
    Correction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnknownCase {
    pub id: UnknownCaseId,
    pub request_summary: String,
    pub status: QueueStatus,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Correction {
    pub id: CorrectionId,
    pub target_id: String,
    pub correction_summary: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueStatus {
    Pending,
    Resolved,
    Dismissed,
}
