#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CapabilityId(String);

impl CapabilityId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskClass {
    ReadOnly,
    ReversibleLocalWrite,
    ExternalSideEffect,
    Destructive,
    Privileged,
}

#[derive(Debug, Clone)]
pub struct CapabilityDescriptor {
    pub id: CapabilityId,
    pub version: u32,
    pub description: String,
    pub risk: RiskClass,
    pub side_effect: bool,
    pub reversible: bool,
}