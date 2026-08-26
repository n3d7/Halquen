use std::collections::BTreeMap;

use halquen_domain::{EntityId, EvidenceId, MemoryId, MemoryRevisionId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Semantic,
    Procedural,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MemoryValue {
    Fact {
        subject: EntityId,
        predicate: String,
        object: String,
    },
    Relation {
        from: EntityId,
        relation: String,
        to: EntityId,
    },
    Preference {
        key: String,
        value: String,
    },
    Procedure {
        name: String,
        capability_ids: Vec<String>,
    },
}

impl MemoryValue {
    pub fn kind(&self) -> MemoryKind {
        match self {
            Self::Fact { .. } | Self::Relation { .. } | Self::Preference { .. } => {
                MemoryKind::Semantic
            }
            Self::Procedure { .. } => MemoryKind::Procedural,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: MemoryId,
    pub kind: MemoryKind,
    pub current_revision_id: MemoryRevisionId,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRevision {
    pub id: MemoryRevisionId,
    pub memory_id: MemoryId,
    pub previous_revision_id: Option<MemoryRevisionId>,
    pub value: MemoryValue,
    pub evidence_ids: Vec<EvidenceId>,
    pub created_at_ms: i64,
    pub valid_from_ms: Option<i64>,
    pub valid_until_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MemoryError {
    #[error("memory change requires at least one evidence reference")]
    MissingEvidence,
    #[error("memory item was not found")]
    ItemNotFound,
    #[error("memory revision was not found")]
    RevisionNotFound,
    #[error("revision does not belong to the requested memory item")]
    RevisionMismatch,
    #[error("memory value does not match the memory item's authoritative kind")]
    KindMismatch,
}

#[derive(Debug, Default)]
pub struct MemoryLedger {
    items: BTreeMap<MemoryId, MemoryItem>,
    revisions: BTreeMap<MemoryRevisionId, MemoryRevision>,
}

impl MemoryLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create(
        &mut self,
        kind: MemoryKind,
        value: MemoryValue,
        evidence_ids: Vec<EvidenceId>,
        now_ms: i64,
    ) -> Result<MemoryId, MemoryError> {
        require_evidence(&evidence_ids)?;
        require_kind(kind, &value)?;
        let memory_id = MemoryId::generate();
        let revision_id = MemoryRevisionId::generate();
        self.revisions.insert(
            revision_id.clone(),
            MemoryRevision {
                id: revision_id.clone(),
                memory_id: memory_id.clone(),
                previous_revision_id: None,
                value,
                evidence_ids,
                created_at_ms: now_ms,
                valid_from_ms: Some(now_ms),
                valid_until_ms: None,
            },
        );
        self.items.insert(
            memory_id.clone(),
            MemoryItem {
                id: memory_id.clone(),
                kind,
                current_revision_id: revision_id,
                created_at_ms: now_ms,
                updated_at_ms: now_ms,
            },
        );
        Ok(memory_id)
    }

    pub fn revise(
        &mut self,
        memory_id: &MemoryId,
        value: MemoryValue,
        evidence_ids: Vec<EvidenceId>,
        now_ms: i64,
    ) -> Result<MemoryRevisionId, MemoryError> {
        require_evidence(&evidence_ids)?;
        let item = self
            .items
            .get_mut(memory_id)
            .ok_or(MemoryError::ItemNotFound)?;
        require_kind(item.kind, &value)?;
        let revision_id = MemoryRevisionId::generate();
        self.revisions.insert(
            revision_id.clone(),
            MemoryRevision {
                id: revision_id.clone(),
                memory_id: memory_id.clone(),
                previous_revision_id: Some(item.current_revision_id.clone()),
                value,
                evidence_ids,
                created_at_ms: now_ms,
                valid_from_ms: Some(now_ms),
                valid_until_ms: None,
            },
        );
        item.current_revision_id = revision_id.clone();
        item.updated_at_ms = now_ms;
        Ok(revision_id)
    }

    pub fn restore(
        &mut self,
        memory_id: &MemoryId,
        source_revision_id: &MemoryRevisionId,
        evidence_ids: Vec<EvidenceId>,
        now_ms: i64,
    ) -> Result<MemoryRevisionId, MemoryError> {
        let source = self
            .revisions
            .get(source_revision_id)
            .ok_or(MemoryError::RevisionNotFound)?;
        if &source.memory_id != memory_id {
            return Err(MemoryError::RevisionMismatch);
        }
        self.revise(memory_id, source.value.clone(), evidence_ids, now_ms)
    }

    pub fn item(&self, id: &MemoryId) -> Option<&MemoryItem> {
        self.items.get(id)
    }

    pub fn revision(&self, id: &MemoryRevisionId) -> Option<&MemoryRevision> {
        self.revisions.get(id)
    }

    pub fn revisions_for(&self, memory_id: &MemoryId) -> Vec<&MemoryRevision> {
        self.revisions
            .values()
            .filter(|revision| &revision.memory_id == memory_id)
            .collect()
    }
}

fn require_evidence(evidence_ids: &[EvidenceId]) -> Result<(), MemoryError> {
    if evidence_ids.is_empty() {
        Err(MemoryError::MissingEvidence)
    } else {
        Ok(())
    }
}

fn require_kind(kind: MemoryKind, value: &MemoryValue) -> Result<(), MemoryError> {
    if kind == value.kind() {
        Ok(())
    } else {
        Err(MemoryError::KindMismatch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preference(value: &str) -> MemoryValue {
        MemoryValue::Preference {
            key: "editor".to_owned(),
            value: value.to_owned(),
        }
    }

    #[test]
    fn revisions_preserve_history_and_evidence() {
        let mut ledger = MemoryLedger::new();
        let first_evidence = EvidenceId::generate();
        let memory_id = ledger
            .create(
                MemoryKind::Semantic,
                preference("VS Code"),
                vec![first_evidence],
                1,
            )
            .unwrap();
        let first_revision = ledger.item(&memory_id).unwrap().current_revision_id.clone();
        let second_revision = ledger
            .revise(
                &memory_id,
                preference("Zed"),
                vec![EvidenceId::generate()],
                2,
            )
            .unwrap();
        assert_eq!(ledger.revisions_for(&memory_id).len(), 2);
        assert_eq!(
            ledger
                .revision(&second_revision)
                .unwrap()
                .previous_revision_id
                .as_ref(),
            Some(&first_revision)
        );
        assert!(ledger.revision(&first_revision).is_some());
    }

    #[test]
    fn restoration_creates_a_new_revision() {
        let mut ledger = MemoryLedger::new();
        let memory_id = ledger
            .create(
                MemoryKind::Semantic,
                preference("VS Code"),
                vec![EvidenceId::generate()],
                1,
            )
            .unwrap();
        let first = ledger.item(&memory_id).unwrap().current_revision_id.clone();
        ledger
            .revise(
                &memory_id,
                preference("Zed"),
                vec![EvidenceId::generate()],
                2,
            )
            .unwrap();
        let restored = ledger
            .restore(&memory_id, &first, vec![EvidenceId::generate()], 3)
            .unwrap();
        assert_ne!(restored, first);
        assert_eq!(ledger.revisions_for(&memory_id).len(), 3);
        assert_eq!(
            ledger.revision(&restored).unwrap().value,
            preference("VS Code")
        );
    }

    #[test]
    fn memory_change_without_evidence_is_rejected() {
        let mut ledger = MemoryLedger::new();
        assert_eq!(
            ledger.create(MemoryKind::Semantic, preference("Zed"), Vec::new(), 1),
            Err(MemoryError::MissingEvidence)
        );
    }

    #[test]
    fn semantic_item_cannot_be_created_with_a_procedure() {
        let mut ledger = MemoryLedger::new();
        let procedure = MemoryValue::Procedure {
            name: "spoofed".to_owned(),
            capability_ids: vec!["system.open_app".to_owned()],
        };
        assert_eq!(
            ledger.create(
                MemoryKind::Semantic,
                procedure,
                vec![EvidenceId::generate()],
                1,
            ),
            Err(MemoryError::KindMismatch)
        );
    }

    #[test]
    fn revision_cannot_change_authoritative_memory_kind() {
        let mut ledger = MemoryLedger::new();
        let memory_id = ledger
            .create(
                MemoryKind::Semantic,
                preference("Zed"),
                vec![EvidenceId::generate()],
                1,
            )
            .unwrap();
        let procedure = MemoryValue::Procedure {
            name: "spoofed".to_owned(),
            capability_ids: vec!["system.open_app".to_owned()],
        };
        assert_eq!(
            ledger.revise(&memory_id, procedure, vec![EvidenceId::generate()], 2,),
            Err(MemoryError::KindMismatch)
        );
        assert_eq!(ledger.revisions_for(&memory_id).len(), 1);
    }
}
