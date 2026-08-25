#![forbid(unsafe_code)]

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

pub mod episodic;
pub mod evidence;
pub mod procedural;
pub mod semantic;

pub use episodic::EpisodicRecord;
pub use evidence::{Evidence, MemoryEvidence};
pub use procedural::{ProceduralCandidate, ProceduralPromotionValidator, PromotionDecision};
pub use semantic::{MemoryError, MemoryItem, MemoryKind, MemoryLedger, MemoryRevision, MemoryValue};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkingValue {
    EntityReference { entity_id: halquen_domain::EntityId },
    TextLabel { value: String },
}

#[derive(Debug)]
pub struct WorkingMemory {
    capacity: usize,
    values: VecDeque<(String, WorkingValue)>,
}

impl WorkingMemory {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            values: VecDeque::with_capacity(capacity.max(1)),
        }
    }

    pub fn set(&mut self, key: impl Into<String>, value: WorkingValue) {
        let key = key.into();
        if let Some(position) = self.values.iter().position(|(stored, _)| stored == &key) {
            self.values.remove(position);
        }
        if self.values.len() == self.capacity {
            self.values.pop_front();
        }
        self.values.push_back((key, value));
    }

    pub fn get(&self, key: &str) -> Option<&WorkingValue> {
        self.values
            .iter()
            .find(|(stored, _)| stored == key)
            .map(|(_, value)| value)
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn working_memory_is_bounded() {
        let mut memory = WorkingMemory::new(2);
        memory.set("first", WorkingValue::TextLabel { value: "1".into() });
        memory.set("second", WorkingValue::TextLabel { value: "2".into() });
        memory.set("third", WorkingValue::TextLabel { value: "3".into() });
        assert_eq!(memory.len(), 2);
        assert!(memory.get("first").is_none());
    }
}
