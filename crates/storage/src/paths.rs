use std::env;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::StorageError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataPaths {
    pub app_dir: PathBuf,
    pub database: PathBuf,
}

impl DataPaths {
    pub fn discover() -> Result<Self, StorageError> {
        let data_home = match env::var_os("XDG_DATA_HOME") {
            Some(value) => PathBuf::from(value),
            None => {
                let home = env::var_os("HOME").ok_or(StorageError::MissingDataHome)?;
                PathBuf::from(home).join(".local/share")
            }
        };
        if !data_home.is_absolute() {
            return Err(StorageError::InsecureDataPath(
                "data home must be absolute".to_owned(),
            ));
        }
        let app_dir = data_home.join("halquen");
        Ok(Self {
            database: app_dir.join("halquen.sqlite3"),
            app_dir,
        })
    }

    pub fn prepare(&self) -> Result<(), StorageError> {
        let parent = self.app_dir.parent().ok_or_else(|| {
            StorageError::InsecureDataPath("application data directory has no parent".to_owned())
        })?;
        match fs::symlink_metadata(parent) {
            Ok(metadata) => validate_owned_directory(parent, &metadata)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir_all(parent)?;
                let metadata = fs::symlink_metadata(parent)?;
                validate_owned_directory(parent, &metadata)?;
            }
            Err(error) => return Err(error.into()),
        }

        match fs::symlink_metadata(&self.app_dir) {
            Ok(metadata) => validate_private_directory(&self.app_dir, &metadata)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&self.app_dir)?;
            }
            Err(error) => return Err(error.into()),
        }

        fs::set_permissions(&self.app_dir, fs::Permissions::from_mode(0o700))?;
        let metadata = fs::symlink_metadata(&self.app_dir)?;
        validate_private_directory(&self.app_dir, &metadata)?;

        match fs::symlink_metadata(&self.database) {
            Ok(metadata)
                if metadata.file_type().is_symlink()
                    || !metadata.is_file()
                    || metadata.uid() != current_uid()? =>
            {
                Err(StorageError::InsecureDataPath(format!(
                    "{} is not a regular user-owned database file",
                    self.database.display()
                )))
            }
            Ok(_) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

fn validate_private_directory(path: &Path, metadata: &fs::Metadata) -> Result<(), StorageError> {
    validate_owned_directory(path, metadata)?;
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(StorageError::InsecureDataPath(format!(
            "{} is accessible by other users",
            path.display()
        )));
    }
    Ok(())
}

fn validate_owned_directory(path: &Path, metadata: &fs::Metadata) -> Result<(), StorageError> {
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(StorageError::InsecureDataPath(format!(
            "{} is not a real directory",
            path.display()
        )));
    }
    let uid = current_uid()?;
    if metadata.uid() != uid {
        return Err(StorageError::InsecureDataPath(format!(
            "{} is not owned by the current user",
            path.display()
        )));
    }
    Ok(())
}

pub(crate) fn current_uid() -> Result<u32, StorageError> {
    let status = fs::read_to_string("/proc/self/status")?;
    let uid_line = status
        .lines()
        .find(|line| line.starts_with("Uid:"))
        .ok_or_else(|| StorageError::InsecureDataPath("cannot determine current UID".to_owned()))?;
    uid_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| StorageError::InsecureDataPath("cannot parse current UID".to_owned()))?
        .parse()
        .map_err(|_| StorageError::InsecureDataPath("cannot parse current UID".to_owned()))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    struct TempTree(PathBuf);

    impl TempTree {
        fn new() -> Self {
            static SEQUENCE: AtomicU64 = AtomicU64::new(0);
            let id = SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "halquen-data-path-test-{}-{id}",
                std::process::id()
            ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn paths(root: &Path) -> DataPaths {
        let app_dir = root.join("data/halquen");
        DataPaths {
            database: app_dir.join("halquen.sqlite3"),
            app_dir,
        }
    }

    #[test]
    fn prepare_creates_a_private_application_directory() {
        let tree = TempTree::new();
        let paths = paths(&tree.0);
        paths.prepare().unwrap();
        let metadata = fs::symlink_metadata(&paths.app_dir).unwrap();
        assert!(metadata.is_dir());
        assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
    }

    #[test]
    fn prepare_rejects_symlinked_or_insecure_application_directory() {
        let tree = TempTree::new();
        let paths = paths(&tree.0);
        fs::create_dir_all(paths.app_dir.parent().unwrap()).unwrap();
        let target = tree.0.join("target");
        fs::create_dir(&target).unwrap();
        symlink(&target, &paths.app_dir).unwrap();
        assert!(paths.prepare().is_err());

        fs::remove_file(&paths.app_dir).unwrap();
        fs::create_dir(&paths.app_dir).unwrap();
        fs::set_permissions(&paths.app_dir, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(paths.prepare().is_err());
    }

    #[test]
    fn prepare_rejects_database_symlink() {
        let tree = TempTree::new();
        let paths = paths(&tree.0);
        paths.prepare().unwrap();
        let target = tree.0.join("other.sqlite3");
        fs::write(&target, b"").unwrap();
        symlink(&target, &paths.database).unwrap();
        assert!(paths.prepare().is_err());
    }
}
