use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

const MAX_ID_LEN: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IdError {
    #[error("identifier cannot be empty")]
    Empty,
    #[error("identifier exceeds {MAX_ID_LEN} bytes")]
    TooLong,
    #[error("identifier contains an unsupported character")]
    InvalidCharacter,
    #[error("capability identifier must use lowercase namespace.operation segments")]
    InvalidCapabilityFormat,
}

fn validate_opaque(value: &str) -> Result<(), IdError> {
    if value.is_empty() {
        return Err(IdError::Empty);
    }
    if value.len() > MAX_ID_LEN {
        return Err(IdError::TooLong);
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'.' | b'_' | b'-'))
    {
        return Err(IdError::InvalidCharacter);
    }
    Ok(())
}

fn valid_capability_segment(segment: &str) -> bool {
    let mut bytes = segment.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn validate_capability(value: &str) -> Result<(), IdError> {
    validate_opaque(value)?;
    let mut segments = value.split('.');
    let namespace = segments.next().unwrap_or_default();
    let operation = segments.next().unwrap_or_default();
    if !valid_capability_segment(namespace)
        || !valid_capability_segment(operation)
        || segments.any(|segment| !valid_capability_segment(segment))
    {
        return Err(IdError::InvalidCapabilityFormat);
    }
    Ok(())
}

macro_rules! typed_id {
    ($name:ident, $validator:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
                let value = value.into();
                $validator(&value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl std::str::FromStr for $name {
            type Err = IdError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

typed_id!(CapabilityId, validate_capability);
typed_id!(EntityId, validate_opaque);
typed_id!(IntentId, validate_opaque);
typed_id!(EventId, validate_opaque);
typed_id!(ExecutionId, validate_opaque);
typed_id!(MemoryId, validate_opaque);
typed_id!(MemoryRevisionId, validate_opaque);
typed_id!(EvidenceId, validate_opaque);
typed_id!(ProposalId, validate_opaque);
typed_id!(CorrectionId, validate_opaque);
typed_id!(UnknownCaseId, validate_opaque);
typed_id!(AuditId, validate_opaque);
typed_id!(ActivityId, validate_opaque);
typed_id!(CacheEntryId, validate_opaque);
typed_id!(ChatMessageId, validate_opaque);
typed_id!(ChatSessionId, validate_opaque);
typed_id!(ModelId, validate_opaque);
typed_id!(ProviderId, validate_opaque);
typed_id!(PermissionId, validate_opaque);
typed_id!(ResourceLabelId, validate_opaque);
typed_id!(AgentId, validate_opaque);
typed_id!(AgentInstanceId, validate_opaque);
typed_id!(AgentSessionId, validate_opaque);
typed_id!(DaemonSessionId, validate_opaque);
typed_id!(BehaviourEventId, validate_opaque);

fn generated_value(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}:{}:{nanos}:{sequence}", std::process::id())
}

macro_rules! generated_id {
    ($name:ident, $prefix:literal) => {
        impl $name {
            pub fn generate() -> Self {
                Self(generated_value($prefix))
            }
        }
    };
}

generated_id!(ExecutionId, "execution");
generated_id!(EventId, "event");
generated_id!(MemoryId, "memory");
generated_id!(MemoryRevisionId, "revision");
generated_id!(EvidenceId, "evidence");
generated_id!(ProposalId, "proposal");
generated_id!(CorrectionId, "correction");
generated_id!(UnknownCaseId, "unknown");
generated_id!(AuditId, "audit");
generated_id!(ActivityId, "activity");
generated_id!(CacheEntryId, "cache");
generated_id!(ChatMessageId, "message");
generated_id!(ChatSessionId, "session");
generated_id!(ModelId, "model");
generated_id!(ProviderId, "provider");
generated_id!(PermissionId, "permission");
generated_id!(ResourceLabelId, "resource-label");
generated_id!(AgentId, "agent");
generated_id!(AgentInstanceId, "agent-instance");
generated_id!(AgentSessionId, "agent-session");
generated_id!(DaemonSessionId, "daemon-session");
generated_id!(BehaviourEventId, "behaviour");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_ids_require_namespace_and_operation() {
        assert!(CapabilityId::new("system.open_app").is_ok());
        assert!(CapabilityId::new("open_app").is_err());
        assert!(CapabilityId::new("System.open_app").is_err());
        assert!(CapabilityId::new("system.shell-execute").is_err());
    }

    #[test]
    fn typed_ids_support_standard_string_parsing() {
    let id: CapabilityId = "system.open_app".parse().unwrap();
    assert_eq!(id.as_str(), "system.open_app");

    assert!("not valid".parse::<CapabilityId>().is_err());
    }

    #[test]
    fn deserialization_validates_ids() {
        let result = serde_json::from_str::<CapabilityId>("\"not valid\"");
        assert!(result.is_err());
    }
}
