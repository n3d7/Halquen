use serde::{Deserialize, Serialize};

use crate::EntityId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Application,
    Project,
    File,
    Person,
    Device,
    Routine,
    Generic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    pub kind: EntityKind,
    pub canonical_name: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub valid_from_ms: Option<i64>,
    pub valid_until_ms: Option<i64>,
}
