use serde::{Deserialize, Serialize};

use crate::{EntityId, SecurityValidationError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutableOwnership {
    RootOnly,
    RootOrCurrentUser,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutableIdentity {
    pub canonical_path: String,
    pub device: u64,
    pub inode: u64,
    pub owner_uid: u32,
    pub size: u64,
    pub modified_seconds: i64,
    pub modified_nanoseconds: i64,
    pub sha256_hex: Option<String>,
}

impl ExecutableIdentity {
    pub fn validate(&self) -> Result<(), SecurityValidationError> {
        if !self.canonical_path.starts_with('/')
            || self.canonical_path.len() > 1_024
            || !(0..1_000_000_000).contains(&self.modified_nanoseconds)
            || self.sha256_hex.as_ref().is_some_and(|hash| {
                hash.len() != 64
                    || !hash
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
            })
        {
            return Err(SecurityValidationError::InvalidExecutableIdentity);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredApplication {
    pub entity_id: EntityId,
    pub display_name: String,
    pub executable: String,
    pub arguments: Vec<String>,
    pub ownership: ExecutableOwnership,
    pub identity: ExecutableIdentity,
    pub enabled: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl RegisteredApplication {
    pub fn validate(&self) -> Result<(), SecurityValidationError> {
        if !self.entity_id.as_str().starts_with("app:")
            || self.display_name.trim().is_empty()
            || self.display_name.len() > 128
            || !self.executable.starts_with('/')
            || self.executable.len() > 1_024
            || self.arguments.len() > 32
            || self
                .arguments
                .iter()
                .any(|argument| argument.len() > 1_024 || argument.as_bytes().contains(&0))
            || self.updated_at_ms < self.created_at_ms
            || self.identity.canonical_path != self.executable
        {
            return Err(SecurityValidationError::InvalidApplicationRegistration);
        }
        self.identity.validate()
    }
}
