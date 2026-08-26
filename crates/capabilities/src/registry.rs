use std::collections::BTreeMap;
use std::fmt;

use halquen_domain::{CapabilityDescriptor, CapabilityId};

#[derive(Debug, PartialEq, Eq)]
pub enum RegistryError {
    AlreadyRegistered(CapabilityId),
    InvalidDescriptor(halquen_domain::CapabilityValidationError),
}

impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyRegistered(id) => {
                write!(formatter, "capability {id} is already registered")
            }
            Self::InvalidDescriptor(error) => write!(formatter, "invalid capability: {error}"),
        }
    }
}

impl std::error::Error for RegistryError {}

pub struct CapabilityRegistry {
    capabilities: BTreeMap<CapabilityId, CapabilityDescriptor>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self {
            capabilities: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, capability: CapabilityDescriptor) -> Result<(), RegistryError> {
        capability
            .validate()
            .map_err(RegistryError::InvalidDescriptor)?;
        let id = capability.id.clone();

        if self.capabilities.contains_key(&id) {
            return Err(RegistryError::AlreadyRegistered(id));
        }

        self.capabilities.insert(id, capability);

        Ok(())
    }

    pub fn get(&self, id: &CapabilityId) -> Option<&CapabilityDescriptor> {
        self.capabilities.get(id)
    }

    pub fn len(&self) -> usize {
        self.capabilities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }

    pub fn list(&self) -> impl ExactSizeIterator<Item = &CapabilityDescriptor> {
        self.capabilities.values()
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use halquen_domain::{ActionArgumentKind, ConfirmationPolicy, RiskClass};

    fn test_capability() -> CapabilityDescriptor {
        CapabilityDescriptor {
            id: CapabilityId::new("system.open_app").unwrap(),
            version: 1,
            description: "Open an application".to_string(),
            risk: RiskClass::LocalSideEffect,
            side_effect: true,
            idempotent: false,
            reversible: false,
            scope_requirements: Vec::new(),
            confirmation: ConfirmationPolicy::RiskBased,
            timeout_ms: 5_000,
            arguments: ActionArgumentKind::OpenApp,
        }
    }

    #[test]
    fn registers_capability() {
        let mut registry = CapabilityRegistry::new();

        registry.register(test_capability()).unwrap();

        let id = CapabilityId::new("system.open_app").unwrap();
        let capability = registry.get(&id).unwrap();

        assert_eq!(capability.id, id);
        assert_eq!(capability.version, 1);
    }

    #[test]
    fn returns_none_for_unknown_capability() {
        let registry = CapabilityRegistry::new();

        let id = CapabilityId::new("system.unknown").unwrap();

        assert!(registry.get(&id).is_none());
    }

    #[test]
    fn rejects_duplicate_capability() {
        let mut registry = CapabilityRegistry::new();

        registry.register(test_capability()).unwrap();

        let result = registry.register(test_capability());

        assert_eq!(
            result,
            Err(RegistryError::AlreadyRegistered(
                CapabilityId::new("system.open_app").unwrap()
            ))
        );
        assert_eq!(registry.len(), 1);
        assert_eq!(
            registry
                .get(&CapabilityId::new("system.open_app").unwrap())
                .unwrap()
                .version,
            1
        );
    }

    #[test]
    fn rejects_invalid_descriptor() {
        let mut registry = CapabilityRegistry::new();
        let mut capability = test_capability();
        capability.version = 0;
        assert!(matches!(
            registry.register(capability),
            Err(RegistryError::InvalidDescriptor(_))
        ));
    }

    #[test]
    fn lists_capabilities_deterministically() {
        let mut registry = CapabilityRegistry::new();
        let mut second = test_capability();
        second.id = CapabilityId::new("alpha.operation").unwrap();
        registry.register(test_capability()).unwrap();
        registry.register(second).unwrap();
        let ids: Vec<_> = registry
            .list()
            .map(|capability| capability.id.as_str())
            .collect();
        assert_eq!(ids, ["alpha.operation", "system.open_app"]);
    }
}
