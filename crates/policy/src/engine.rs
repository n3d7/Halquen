use halquen_domain::{
    CapabilityDescriptor,
    RiskClass,
};

use crate::PolicyDecision;

/*  */
pub struct PolicyEngine;

impl PolicyEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn evaluate(
        &self,
        capability: &CapabilityDescriptor,
    ) -> PolicyDecision {
        match capability.risk {
            RiskClass::ReadOnly => PolicyDecision::Allow,

            RiskClass::ReversibleLocalWrite => {
                PolicyDecision::Allow
            }

            RiskClass::ExternalSideEffect => {
                PolicyDecision::Confirm
            }

            RiskClass::Destructive => {
                PolicyDecision::Confirm
            }

            RiskClass::Privileged => {
                PolicyDecision::Deny
            }
        }
    }
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}