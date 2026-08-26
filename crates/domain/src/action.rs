use crate::{CapabilityId, EntityId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionArgumentKind {
    None,
    OpenApp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionArguments {
    None,

    OpenApp { app: EntityId },
}

impl ActionArguments {
    pub fn kind(&self) -> ActionArgumentKind {
        match self {
            Self::None => ActionArgumentKind::None,
            Self::OpenApp { .. } => ActionArgumentKind::OpenApp,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionRequest {
    pub capability_id: CapabilityId,
    pub arguments: ActionArguments,
}

impl ActionRequest {
    pub fn new(capability_id: CapabilityId, arguments: ActionArguments) -> Self {
        Self {
            capability_id,
            arguments,
        }
    }
}
