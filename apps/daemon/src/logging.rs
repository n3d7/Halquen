use std::env;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use halquen_domain::{ApplicationSettings, LogLevel};
use thiserror::Error;
use tracing_appender::non_blocking::WorkerGuard;

#[derive(Debug, Error)]
pub enum LoggingError {
    #[error("XDG_STATE_HOME is unset and HOME is unavailable")]
    MissingStateHome,
    #[error("log directory is not private and user-owned")]
    InsecureLogDirectory,
    #[error("cannot determine current user")]
    UnknownUser,
    #[error("logging filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("logging subscriber is already configured")]
    SubscriberAlreadySet,
}

pub fn initialize(settings: &ApplicationSettings) -> Result<WorkerGuard, LoggingError> {
    let directory = prepare_log_directory()?;
    let policy = logging_policy(settings);
    prune_logs(&directory, policy.retention, policy.max_total_bytes)?;
    let file = tracing_appender::rolling::daily(&directory, "halquen.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file);
    tracing_subscriber::fmt()
        .json()
        .with_ansi(false)
        .with_max_level(policy.level)
        .with_writer(non_blocking)
        .try_init()
        .map_err(|_| LoggingError::SubscriberAlreadySet)?;
    Ok(guard)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LoggingPolicy {
    level: tracing::Level,
    retention: Duration,
    max_total_bytes: u64,
}

fn logging_policy(settings: &ApplicationSettings) -> LoggingPolicy {
    let configured_level = match settings.log_level {
        LogLevel::Error => tracing::Level::ERROR,
        LogLevel::Warn => tracing::Level::WARN,
        LogLevel::Info => tracing::Level::INFO,
        LogLevel::Debug => tracing::Level::DEBUG,
    };
    LoggingPolicy {
        level: if settings.diagnostic_logging {
            configured_level
        } else {
            tracing::Level::ERROR
        },
        retention: Duration::from_secs(u64::from(settings.log_retention_days) * 24 * 60 * 60),
        max_total_bytes: u64::from(settings.log_max_total_mb) * 1024 * 1024,
    }
}

pub fn redact(value: &str) -> String {
    let mut redact_next = 0_u8;
    let mut output = Vec::new();
    for token in value.split_whitespace() {
        let lower = token.to_ascii_lowercase();
        let sensitive = lower.contains("api_key")
            || lower.contains("apikey")
            || lower.contains("password")
            || lower.contains("authorization")
            || lower == "bearer";
        if redact_next > 0 || sensitive {
            output.push("[REDACTED]");
            redact_next = redact_next.saturating_sub(1);
            if lower.contains("authorization") {
                redact_next = 2;
            } else if lower == "bearer" {
                redact_next = 1;
            }
        } else if token.len() > 256 {
            output.push("[OVERSIZED_REDACTED]");
        } else {
            output.push(token);
        }
    }
    output.join(" ")
}

pub(crate) fn clear_historical_logs() -> Result<u64, LoggingError> {
    let directory = prepare_log_directory()?;
    let removed = clear_historical_logs_in(&directory)?;
    u64::try_from(removed).map_err(|_| {
        LoggingError::Io(std::io::Error::other(
            "historical log count exceeded supported bounds",
        ))
    })
}

fn clear_historical_logs_in(directory: &Path) -> Result<usize, LoggingError> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .file_name()
                .to_string_lossy()
                .starts_with("halquen.log")
        {
            files.push(entry.path());
        }
    }
    files.sort();

    // The daily appender's lexicographically newest file is the active log. Keep it so
    // clearing history never unlinks the file that the logging worker is still writing.
    files.pop();
    let removed = files.len();
    for path in files {
        fs::remove_file(path)?;
    }
    Ok(removed)
}

fn prepare_log_directory() -> Result<PathBuf, LoggingError> {
    let state_home = match env::var_os("XDG_STATE_HOME") {
        Some(value) => PathBuf::from(value),
        None => PathBuf::from(env::var_os("HOME").ok_or(LoggingError::MissingStateHome)?)
            .join(".local/state"),
    };
    if !state_home.is_absolute() {
        return Err(LoggingError::InsecureLogDirectory);
    }
    let directory = state_home.join("halquen/logs");
    fs::create_dir_all(&directory)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    let metadata = fs::symlink_metadata(&directory)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != current_uid()?
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(LoggingError::InsecureLogDirectory);
    }
    Ok(directory)
}

fn prune_logs(
    directory: &Path,
    retention: Duration,
    max_total_bytes: u64,
) -> Result<(), LoggingError> {
    let now = SystemTime::now();
    let mut files = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            (metadata.is_file()
                && entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("halquen.log"))
            .then_some((entry.path(), metadata.len(), metadata.modified().ok()?))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|(_, _, modified)| *modified);
    let mut total = files.iter().map(|(_, size, _)| *size).sum::<u64>();
    for (path, size, modified) in files {
        let expired = now
            .duration_since(modified)
            .is_ok_and(|age| age > retention);
        if expired || total > max_total_bytes {
            fs::remove_file(path)?;
            total = total.saturating_sub(size);
        }
    }
    Ok(())
}

fn current_uid() -> Result<u32, LoggingError> {
    let status = fs::read_to_string("/proc/self/status")?;
    status
        .lines()
        .find(|line| line.starts_with("Uid:"))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .ok_or(LoggingError::UnknownUser)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn representative_credentials_are_redacted() {
        let text = redact(
            "Authorization: Bearer TEST_TOKEN api_key=TEST_KEY password=TEST_PASSWORD safe=value",
        );
        assert!(!text.contains("TEST_TOKEN"));
        assert!(!text.contains("TEST_KEY"));
        assert!(!text.contains("TEST_PASSWORD"));
        assert!(text.contains("safe=value"));
    }

    #[test]
    fn logging_policy_uses_validated_application_settings() {
        let mut settings = ApplicationSettings {
            log_level: LogLevel::Debug,
            log_retention_days: 12,
            log_max_total_mb: 48,
            ..ApplicationSettings::default()
        };
        let policy = logging_policy(&settings);
        assert_eq!(policy.level, tracing::Level::DEBUG);
        assert_eq!(policy.retention, Duration::from_secs(12 * 24 * 60 * 60));
        assert_eq!(policy.max_total_bytes, 48 * 1024 * 1024);

        settings.diagnostic_logging = false;
        assert_eq!(logging_policy(&settings).level, tracing::Level::ERROR);
    }

    #[test]
    fn clearing_history_preserves_active_log_and_unrelated_files() {
        let directory =
            std::env::temp_dir().join(format!("halquen-log-cleanup-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("halquen.log.2026-08-24"), b"old").unwrap();
        fs::write(directory.join("halquen.log.2026-08-25"), b"current").unwrap();
        fs::write(directory.join("notes.txt"), b"unrelated").unwrap();

        assert_eq!(clear_historical_logs_in(&directory).unwrap(), 1);
        assert!(!directory.join("halquen.log.2026-08-24").exists());
        assert!(directory.join("halquen.log.2026-08-25").exists());
        assert!(directory.join("notes.txt").exists());

        fs::remove_dir_all(directory).unwrap();
    }
}
