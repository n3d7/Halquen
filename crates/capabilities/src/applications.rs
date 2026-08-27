use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use halquen_domain::{EntityId, RegisteredApplication};
use thiserror::Error;

pub type SharedApplicationRegistry = Arc<RwLock<ApplicationRegistry>>;

#[derive(Debug, Default)]
pub struct ApplicationRegistry {
    applications: BTreeMap<EntityId, RegisteredApplication>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ApplicationRegistryError {
    #[error("application registration is invalid")]
    InvalidRegistration,
}

impl ApplicationRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_applications(
        applications: impl IntoIterator<Item = RegisteredApplication>,
    ) -> Result<Self, ApplicationRegistryError> {
        let mut registry = Self::new();
        for application in applications {
            registry.upsert(application)?;
        }
        Ok(registry)
    }

    pub fn upsert(
        &mut self,
        application: RegisteredApplication,
    ) -> Result<(), ApplicationRegistryError> {
        application
            .validate()
            .map_err(|_| ApplicationRegistryError::InvalidRegistration)?;
        self.applications
            .insert(application.entity_id.clone(), application);
        Ok(())
    }

    pub fn remove(&mut self, entity_id: &EntityId) -> bool {
        self.applications.remove(entity_id).is_some()
    }

    pub fn get(&self, entity_id: &EntityId) -> Option<&RegisteredApplication> {
        self.applications.get(entity_id)
    }

    pub fn list(&self) -> impl ExactSizeIterator<Item = &RegisteredApplication> {
        self.applications.values()
    }
}
