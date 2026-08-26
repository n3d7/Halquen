use std::env;
use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePaths {
    pub app_dir: PathBuf,
    pub socket: PathBuf,
}

#[derive(Debug, Error)]
pub enum RuntimePathError {
    #[error("XDG_RUNTIME_DIR is required for secure local IPC")]
    MissingRuntimeDirectory,
    #[error("insecure runtime path: {0}")]
    Insecure(String),
    #[error("runtime filesystem error: {0}")]
    Io(#[from] std::io::Error),
}

impl RuntimePaths {
    pub fn discover() -> Result<Self, RuntimePathError> {
        let root = env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .ok_or(RuntimePathError::MissingRuntimeDirectory)?;
        if !root.is_absolute() {
            return Err(RuntimePathError::Insecure(
                "XDG_RUNTIME_DIR must be absolute".to_owned(),
            ));
        }
        let app_dir = root.join("halquen");
        Ok(Self {
            socket: app_dir.join("halquen.sock"),
            app_dir,
        })
    }

    pub fn prepare_server(&self) -> Result<(), RuntimePathError> {
        let root = self.app_dir.parent().ok_or_else(|| {
            RuntimePathError::Insecure("runtime application directory has no parent".to_owned())
        })?;
        validate_directory(root, true)?;

        match fs::symlink_metadata(&self.app_dir) {
            Ok(metadata) => validate_directory_metadata(&self.app_dir, &metadata, true)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&self.app_dir)?;
            }
            Err(error) => return Err(error.into()),
        }
        fs::set_permissions(&self.app_dir, fs::Permissions::from_mode(0o700))?;
        validate_directory(&self.app_dir, true)?;

        match fs::symlink_metadata(&self.socket) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink()
                    || !metadata.file_type().is_socket()
                    || metadata.uid() != current_uid()?
                {
                    return Err(RuntimePathError::Insecure(format!(
                        "refusing to replace {}",
                        self.socket.display()
                    )));
                }
                fs::remove_file(&self.socket)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }

    pub fn secure_bound_socket(&self) -> Result<(), RuntimePathError> {
        let metadata = fs::symlink_metadata(&self.socket)?;
        if !metadata.file_type().is_socket()
            || metadata.file_type().is_symlink()
            || metadata.uid() != current_uid()?
        {
            return Err(RuntimePathError::Insecure(
                "bound socket path is not a user-owned Unix socket".to_owned(),
            ));
        }
        fs::set_permissions(&self.socket, fs::Permissions::from_mode(0o600))?;
        let secured = fs::symlink_metadata(&self.socket)?;
        if secured.permissions().mode() & 0o077 != 0 {
            return Err(RuntimePathError::Insecure(
                "bound socket permissions are not private".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn validate_client(&self) -> Result<(), RuntimePathError> {
        validate_directory(&self.app_dir, true)?;
        let metadata = fs::symlink_metadata(&self.socket)?;
        if metadata.file_type().is_symlink()
            || !metadata.file_type().is_socket()
            || metadata.uid() != current_uid()?
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(RuntimePathError::Insecure(format!(
                "{} is not a private user-owned Unix socket",
                self.socket.display()
            )));
        }
        Ok(())
    }
}

fn validate_directory(path: &Path, private: bool) -> Result<(), RuntimePathError> {
    let metadata = fs::symlink_metadata(path)?;
    validate_directory_metadata(path, &metadata, private)
}

fn validate_directory_metadata(
    path: &Path,
    metadata: &fs::Metadata,
    private: bool,
) -> Result<(), RuntimePathError> {
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(RuntimePathError::Insecure(format!(
            "{} is not a real directory",
            path.display()
        )));
    }
    if metadata.uid() != current_uid()? {
        return Err(RuntimePathError::Insecure(format!(
            "{} is not owned by the current user",
            path.display()
        )));
    }
    if private && metadata.permissions().mode() & 0o077 != 0 {
        return Err(RuntimePathError::Insecure(format!(
            "{} is accessible by other users",
            path.display()
        )));
    }
    Ok(())
}

fn current_uid() -> Result<u32, RuntimePathError> {
    let status = fs::read_to_string("/proc/self/status")?;
    let value = status
        .lines()
        .find(|line| line.starts_with("Uid:"))
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or_else(|| RuntimePathError::Insecure("cannot determine current UID".to_owned()))?;
    value
        .parse()
        .map_err(|_| RuntimePathError::Insecure("cannot parse current UID".to_owned()))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;
    use std::os::unix::net::UnixListener;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    struct TempRuntime(PathBuf);

    impl TempRuntime {
        fn new() -> Self {
            static SEQUENCE: AtomicU64 = AtomicU64::new(0);
            let id = SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "halquen-runtime-path-test-{}-{id}",
                std::process::id()
            ));
            fs::create_dir(&root).unwrap();
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
            Self(root)
        }

        fn paths(&self) -> RuntimePaths {
            let app_dir = self.0.join("halquen");
            RuntimePaths {
                socket: app_dir.join("halquen.sock"),
                app_dir,
            }
        }
    }

    impl Drop for TempRuntime {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn stale_owned_socket_is_removed_before_bind() {
        let runtime = TempRuntime::new();
        let paths = runtime.paths();
        paths.prepare_server().unwrap();
        let listener = match UnixListener::bind(&paths.socket) {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
            Err(error) => panic!("failed to bind test socket: {error}"),
        };
        drop(listener);
        paths.prepare_server().unwrap();
        assert!(!paths.socket.exists());
    }

    #[test]
    fn socket_symlink_is_never_replaced() {
        let runtime = TempRuntime::new();
        let paths = runtime.paths();
        paths.prepare_server().unwrap();
        let target = runtime.0.join("target");
        fs::write(&target, b"").unwrap();
        symlink(&target, &paths.socket).unwrap();
        assert!(paths.prepare_server().is_err());
        assert!(
            fs::symlink_metadata(&paths.socket)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn client_rejects_public_socket_permissions() {
        let runtime = TempRuntime::new();
        let paths = runtime.paths();
        paths.prepare_server().unwrap();
        let listener = match UnixListener::bind(&paths.socket) {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
            Err(error) => panic!("failed to bind test socket: {error}"),
        };
        paths.secure_bound_socket().unwrap();
        fs::set_permissions(&paths.socket, fs::Permissions::from_mode(0o666)).unwrap();
        assert!(paths.validate_client().is_err());
        drop(listener);
    }

    #[test]
    fn server_rejects_public_application_directory() {
        let runtime = TempRuntime::new();
        let paths = runtime.paths();
        fs::create_dir(&paths.app_dir).unwrap();
        fs::set_permissions(&paths.app_dir, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(paths.prepare_server().is_err());
    }
}
