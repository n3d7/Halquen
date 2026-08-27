use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

use halquen_domain::{ExecutableIdentity, ExecutableOwnership};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExecutableIdentityError {
    #[error("executable path must be absolute and canonical")]
    InvalidPath,
    #[error("executable or one of its parent components is a symbolic link")]
    SymlinkComponent,
    #[error("executable is not a regular executable file")]
    NotExecutable,
    #[error("executable ownership does not satisfy the configured policy")]
    InvalidOwner,
    #[error("executable or parent directory is writable by group or other users")]
    InsecurePermissions,
    #[error("executable content hash does not match the configured pin")]
    HashMismatch,
    #[error("executable identity changed after registration")]
    IdentityChanged,
    #[error("executable identity could not be inspected")]
    InspectionFailed,
}

pub fn inspect_executable(
    value: &str,
    ownership: ExecutableOwnership,
    sha256_pin: Option<&str>,
) -> Result<ExecutableIdentity, ExecutableIdentityError> {
    let path = Path::new(value);
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(ExecutableIdentityError::InvalidPath);
    }
    reject_symlink_components(path)?;
    let canonical = path
        .canonicalize()
        .map_err(|_| ExecutableIdentityError::InspectionFailed)?;
    if canonical != path {
        return Err(ExecutableIdentityError::InvalidPath);
    }
    let metadata =
        fs::metadata(&canonical).map_err(|_| ExecutableIdentityError::InspectionFailed)?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return Err(ExecutableIdentityError::NotExecutable);
    }
    let current_uid = current_uid()?;
    let root_owner_uid = fs::metadata("/")
        .map_err(|_| ExecutableIdentityError::InspectionFailed)?
        .uid();
    validate_owner(metadata.uid(), ownership, current_uid, root_owner_uid)?;
    if metadata.permissions().mode() & 0o022 != 0 {
        return Err(ExecutableIdentityError::InsecurePermissions);
    }
    validate_parent_directories(&canonical, ownership, current_uid, root_owner_uid)?;

    let sha256_hex = match sha256_pin {
        Some(expected) => {
            if expected.len() != 64
                || !expected
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
            {
                return Err(ExecutableIdentityError::HashMismatch);
            }
            let actual = sha256(&canonical)?;
            if actual != expected {
                return Err(ExecutableIdentityError::HashMismatch);
            }
            Some(actual)
        }
        None => None,
    };

    Ok(ExecutableIdentity {
        canonical_path: canonical.to_string_lossy().into_owned(),
        device: metadata.dev(),
        inode: metadata.ino(),
        owner_uid: metadata.uid(),
        size: metadata.size(),
        modified_seconds: metadata.mtime(),
        modified_nanoseconds: metadata.mtime_nsec(),
        sha256_hex,
    })
}

pub fn verify_executable(
    expected: &ExecutableIdentity,
    ownership: ExecutableOwnership,
) -> Result<PathBuf, ExecutableIdentityError> {
    expected
        .validate()
        .map_err(|_| ExecutableIdentityError::IdentityChanged)?;
    let actual = inspect_executable(
        &expected.canonical_path,
        ownership,
        expected.sha256_hex.as_deref(),
    )?;
    if &actual != expected {
        return Err(ExecutableIdentityError::IdentityChanged);
    }
    Ok(PathBuf::from(&actual.canonical_path))
}

fn reject_symlink_components(path: &Path) -> Result<(), ExecutableIdentityError> {
    let mut current = PathBuf::from("/");
    for component in path.components().skip(1) {
        current.push(component.as_os_str());
        let metadata = fs::symlink_metadata(&current)
            .map_err(|_| ExecutableIdentityError::InspectionFailed)?;
        if metadata.file_type().is_symlink() {
            return Err(ExecutableIdentityError::SymlinkComponent);
        }
    }
    Ok(())
}

fn validate_parent_directories(
    executable: &Path,
    ownership: ExecutableOwnership,
    current_uid: u32,
    root_owner_uid: u32,
) -> Result<(), ExecutableIdentityError> {
    for parent in executable.ancestors().skip(1) {
        let metadata =
            fs::symlink_metadata(parent).map_err(|_| ExecutableIdentityError::InspectionFailed)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(ExecutableIdentityError::SymlinkComponent);
        }
        validate_owner(metadata.uid(), ownership, current_uid, root_owner_uid)?;
        if metadata.permissions().mode() & 0o022 != 0 {
            return Err(ExecutableIdentityError::InsecurePermissions);
        }
    }
    Ok(())
}

fn validate_owner(
    owner_uid: u32,
    ownership: ExecutableOwnership,
    current_uid: u32,
    root_owner_uid: u32,
) -> Result<(), ExecutableIdentityError> {
    let valid = match ownership {
        ExecutableOwnership::RootOnly => owner_uid == root_owner_uid,
        ExecutableOwnership::RootOrCurrentUser => {
            owner_uid == root_owner_uid || owner_uid == current_uid
        }
    };
    if valid {
        Ok(())
    } else {
        Err(ExecutableIdentityError::InvalidOwner)
    }
}

fn current_uid() -> Result<u32, ExecutableIdentityError> {
    let status = fs::read_to_string("/proc/self/status")
        .map_err(|_| ExecutableIdentityError::InspectionFailed)?;
    status
        .lines()
        .find(|line| line.starts_with("Uid:"))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .ok_or(ExecutableIdentityError::InspectionFailed)
}

fn sha256(path: &Path) -> Result<String, ExecutableIdentityError> {
    let mut file = File::open(path).map_err(|_| ExecutableIdentityError::InspectionFailed)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 32 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| ExecutableIdentityError::InspectionFailed)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_executable_can_be_pinned_and_verified() {
        let Some(path) = ["/usr/bin/true", "/usr/bin/printf"]
            .into_iter()
            .find(|candidate| Path::new(candidate).is_file())
        else {
            return;
        };
        let identity = inspect_executable(path, ExecutableOwnership::RootOnly, None).unwrap();
        assert_eq!(
            verify_executable(&identity, ExecutableOwnership::RootOnly).unwrap(),
            PathBuf::from(path)
        );
    }

    #[test]
    fn symlink_alias_is_rejected() {
        let Some(path) = ["/bin/true", "/bin/printf"]
            .into_iter()
            .find(|candidate| Path::new(candidate).is_file())
        else {
            return;
        };
        if fs::symlink_metadata("/bin").is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            assert!(matches!(
                inspect_executable(path, ExecutableOwnership::RootOnly, None),
                Err(ExecutableIdentityError::SymlinkComponent)
            ));
        }
    }

    #[test]
    fn replacement_and_incorrect_hash_pins_fail_closed() {
        let Some(path) = ["/usr/bin/true", "/usr/bin/printf"]
            .into_iter()
            .find(|candidate| Path::new(candidate).is_file())
        else {
            return;
        };
        let mut identity = inspect_executable(path, ExecutableOwnership::RootOnly, None).unwrap();
        identity.size = identity.size.saturating_add(1);
        assert!(matches!(
            verify_executable(&identity, ExecutableOwnership::RootOnly),
            Err(ExecutableIdentityError::IdentityChanged)
        ));
        assert!(matches!(
            inspect_executable(path, ExecutableOwnership::RootOnly, Some(&"0".repeat(64)),),
            Err(ExecutableIdentityError::HashMismatch)
        ));
    }
}
