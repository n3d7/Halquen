use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ActionArgumentKind, CapabilityId, IdError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskClass {
    ReadOnly,
    LocalSideEffect,
    ReversibleLocalWrite,
    ExternalSideEffect,
    Destructive,
    Privileged,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmationPolicy {
    RiskBased,
    Always,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScopeRequirement {
    Application { entity: crate::EntityId },
    PathPrefix { path: String },
    Named { scope: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityDescriptor {
    pub id: CapabilityId,
    pub version: u32,
    pub description: String,
    pub risk: RiskClass,
    pub side_effect: bool,
    pub idempotent: bool,
    pub reversible: bool,
    pub scope_requirements: Vec<ScopeRequirement>,
    pub confirmation: ConfirmationPolicy,
    pub timeout_ms: u64,
    pub arguments: ActionArgumentKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CapabilityValidationError {
    #[error("invalid capability identifier: {0}")]
    InvalidId(#[from] IdError),
    #[error("capability version must be greater than zero")]
    ZeroVersion,
    #[error("capability description cannot be empty or exceed 512 bytes")]
    InvalidDescription,
    #[error("capability timeout must be between 1 and 300000 milliseconds")]
    InvalidTimeout,
    #[error("read-only capabilities cannot declare side effects")]
    ReadOnlySideEffect,
    #[error("read-only capabilities cannot be reversible")]
    ReadOnlyReversible,
    #[error("local side-effect capabilities must declare a non-reversible side effect")]
    InvalidLocalSideEffect,
    #[error("reversible-local-write capabilities must declare a reversible side effect")]
    InvalidReversibleLocalWrite,
    #[error("external-side-effect and destructive capabilities must declare a side effect")]
    RiskRequiresSideEffect,
    #[error("destructive capabilities cannot claim reversibility")]
    DestructiveReversible,
    #[error("reversible capabilities must declare a side effect")]
    ReversibleWithoutSideEffect,
}

impl CapabilityDescriptor {
    pub fn validate(&self) -> Result<(), CapabilityValidationError> {
        CapabilityId::new(self.id.as_str())?;
        if self.version == 0 {
            return Err(CapabilityValidationError::ZeroVersion);
        }
        if self.description.trim().is_empty() || self.description.len() > 512 {
            return Err(CapabilityValidationError::InvalidDescription);
        }
        if !(1..=300_000).contains(&self.timeout_ms) {
            return Err(CapabilityValidationError::InvalidTimeout);
        }
        if self.risk == RiskClass::ReadOnly && self.side_effect {
            return Err(CapabilityValidationError::ReadOnlySideEffect);
        }
        if self.risk == RiskClass::ReadOnly && self.reversible {
            return Err(CapabilityValidationError::ReadOnlyReversible);
        }
        if self.risk == RiskClass::LocalSideEffect && (!self.side_effect || self.reversible) {
            return Err(CapabilityValidationError::InvalidLocalSideEffect);
        }
        if self.risk == RiskClass::ReversibleLocalWrite
            && (!self.side_effect || !self.reversible)
        {
            return Err(CapabilityValidationError::InvalidReversibleLocalWrite);
        }
        if matches!(self.risk, RiskClass::ExternalSideEffect | RiskClass::Destructive)
            && !self.side_effect
        {
            return Err(CapabilityValidationError::RiskRequiresSideEffect);
        }
        if self.risk == RiskClass::Destructive && self.reversible {
            return Err(CapabilityValidationError::DestructiveReversible);
        }
        if self.reversible && !self.side_effect {
            return Err(CapabilityValidationError::ReversibleWithoutSideEffect);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(risk: RiskClass, side_effect: bool, reversible: bool) -> CapabilityDescriptor {
        CapabilityDescriptor {
            id: CapabilityId::new("test.operation").unwrap(),
            version: 1,
            description: "test".to_owned(),
            risk,
            side_effect,
            idempotent: false,
            reversible,
            scope_requirements: Vec::new(),
            confirmation: ConfirmationPolicy::RiskBased,
            timeout_ms: 100,
            arguments: ActionArgumentKind::None,
        }
    }

    #[test]
    fn rejects_inconsistent_reversible_local_write() {
        assert_eq!(
            descriptor(RiskClass::ReversibleLocalWrite, true, false).validate(),
            Err(CapabilityValidationError::InvalidReversibleLocalWrite)
        );
    }

    #[test]
    fn accepts_non_reversible_local_side_effect() {
        assert!(descriptor(RiskClass::LocalSideEffect, true, false).validate().is_ok());
    }

    #[test]
    fn rejects_risk_classes_with_impossible_effect_metadata() {
        assert!(descriptor(RiskClass::ReadOnly, false, true).validate().is_err());
        assert!(descriptor(RiskClass::LocalSideEffect, false, false).validate().is_err());
        assert!(descriptor(RiskClass::LocalSideEffect, true, true).validate().is_err());
        assert!(descriptor(RiskClass::ExternalSideEffect, false, false).validate().is_err());
        assert!(descriptor(RiskClass::Destructive, true, true).validate().is_err());
    }
}
