use keyring::v1::Entry;
use thiserror::Error;
use zeroize::Zeroizing;

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("OS credential storage is unavailable")]
    Unavailable,
    #[error("credential was not found")]
    NotFound,
    #[error("credential operation failed")]
    OperationFailed,
}

pub trait SecretStore: Send + Sync {
    fn store(&self, credential_id: &str, secret: Zeroizing<String>) -> Result<(), SecretError>;
    fn retrieve(&self, credential_id: &str) -> Result<Zeroizing<String>, SecretError>;
    fn delete(&self, credential_id: &str) -> Result<(), SecretError>;
}

#[derive(Debug, Clone)]
pub struct KeyringSecretStore {
    service: String,
}

impl KeyringSecretStore {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, credential_id: &str) -> Result<Entry, SecretError> {
        Entry::new(&self.service, credential_id).map_err(|_| SecretError::Unavailable)
    }
}

impl SecretStore for KeyringSecretStore {
    fn store(&self, credential_id: &str, secret: Zeroizing<String>) -> Result<(), SecretError> {
        self.entry(credential_id)?
            .set_password(&secret)
            .map_err(|_| SecretError::OperationFailed)
    }

    fn retrieve(&self, credential_id: &str) -> Result<Zeroizing<String>, SecretError> {
        self.entry(credential_id)?
            .get_password()
            .map(Zeroizing::new)
            .map_err(|error| match error {
                keyring::Error::NoEntry => SecretError::NotFound,
                _ => SecretError::OperationFailed,
            })
    }

    fn delete(&self, credential_id: &str) -> Result<(), SecretError> {
        self.entry(credential_id)?
            .delete_credential()
            .map_err(|error| match error {
                keyring::Error::NoEntry => SecretError::NotFound,
                _ => SecretError::OperationFailed,
            })
    }
}
