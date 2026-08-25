use std::collections::HashMap;

use halquen_domain::{
    CapabilityDescriptor,
    CapabilityId,
};

#[derive(Debug, PartialEq, Eq)]
pub enum RegistryError {
    AlreadyRegistered(CapabilityId),
}

pub struct CapabilityRegistry {
    capabilities: HashMap<CapabilityId, CapabilityDescriptor>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self {
            capabilities: HashMap::new(),
        }
    }

    pub fn register(
        &mut self,
        capability: CapabilityDescriptor,
    ) -> Result<(), RegistryError> {
        let id = capability.id.clone();

        if self.capabilities.contains_key(&id) {
            return Err(RegistryError::AlreadyRegistered(id));
        }

        self.capabilities.insert(id, capability);

        Ok(())
    }

    pub fn get(
        &self,
        id: &CapabilityId,
    ) -> Option<&CapabilityDescriptor> {
        self.capabilities.get(id)
    }

    pub fn len(&self) -> usize {
        self.capabilities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
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

    use halquen_domain::RiskClass;

    fn test_capability() -> CapabilityDescriptor {
        CapabilityDescriptor {
            id: CapabilityId::new("system.open_app"),
            version: 1,
            description: "Open an application".to_string(),
            risk: RiskClass::ReversibleLocalWrite,
            side_effect: true,
            reversible: false,
        }
    }

    #[test]
    fn registers_capability() {
        let mut registry = CapabilityRegistry::new();

        registry.register(test_capability()).unwrap();

        let id = CapabilityId::new("system.open_app");
        let capability = registry.get(&id).unwrap();

        assert_eq!(capability.id, id);
        assert_eq!(capability.version, 1);
    }

    #[test]
    fn returns_none_for_unknown_capability() {
        let registry = CapabilityRegistry::new();

        let id = CapabilityId::new("system.unknown");

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
                CapabilityId::new("system.open_app")
            ))
        );
    }
}