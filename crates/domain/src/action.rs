use crate::{
    CapabilityId,
    EntityId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionArguments {
    None,

    OpenApp {
        app: EntityId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionRequest {
    pub capability_id: CapabilityId,
    pub arguments: ActionArguments,
}

impl ActionRequest {
    pub fn new(
        capability_id: CapabilityId,
        arguments: ActionArguments,
    ) -> Self {
        Self {
            capability_id,
            arguments,
        }
    }
}