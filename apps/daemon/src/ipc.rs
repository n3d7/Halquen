use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use halquen_capabilities::{
    ApplicationRegistry, DryRunExecutor, RealLinuxExecutor, RuntimeExecutor,
};
use halquen_protocol::{
    CodecError, MAX_FRAME_SIZE, PROTOCOL_VERSION, ProtocolErrorBody, ProtocolErrorCode,
    ProtocolRequest, ProtocolResponse, RequestEnvelope, ResponseEnvelope, RuntimePaths,
    decode_request, encode_response,
};
use halquen_storage::{DataPaths, Database};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::{Mutex, watch};
use tokio::time::timeout;

use crate::service::HalquenService;

const IO_TIMEOUT: Duration = Duration::from_secs(5);
type SharedService = Arc<Mutex<HalquenService<RuntimeExecutor>>>;
type CancellationRegistry = Arc<Mutex<BTreeMap<String, watch::Sender<bool>>>>;

#[derive(Debug, Error)]
pub enum DaemonError {
    #[error("runtime path error: {0}")]
    Runtime(#[from] halquen_protocol::RuntimePathError),
    #[error("storage error: {0}")]
    Storage(#[from] halquen_storage::StorageError),
    #[error("IPC error: {0}")]
    Io(#[from] std::io::Error),
    #[error("protocol error: {0}")]
    Protocol(#[from] CodecError),
    #[error("IPC operation timed out")]
    Timeout,
    #[error("request frame exceeded the maximum size")]
    FrameTooLarge,
    #[error("request contained trailing bytes after the first frame")]
    TrailingBytes,
    #[error("peer disconnected before terminating the request frame")]
    IncompleteFrame,
    #[error("failed to initialize core: {0}")]
    Initialization(String),
    #[error("failed to initialize operational logging: {0}")]
    Logging(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecutionMode {
    #[default]
    DryRun,
    Real,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DaemonOptions {
    pub execution_mode: ExecutionMode,
    pub allow_unsafe_agents: bool,
}

pub async fn run(options: DaemonOptions) -> Result<(), DaemonError> {
    let data_paths = DataPaths::discover()?;
    data_paths.prepare()?;
    let database = Database::open(&data_paths.database)?;
    let settings = database.application_settings()?;
    let _log_guard = crate::logging::initialize(&settings)
        .map_err(|error| DaemonError::Logging(error.to_string()))?;
    let applications = Arc::new(std::sync::RwLock::new(
        ApplicationRegistry::from_applications(database.list_registered_applications(200)?)
            .map_err(|error| DaemonError::Initialization(error.to_string()))?,
    ));
    let executor = match options.execution_mode {
        ExecutionMode::DryRun => RuntimeExecutor::DryRun(DryRunExecutor::new()),
        ExecutionMode::Real => {
            RuntimeExecutor::RealLinux(RealLinuxExecutor::new(Arc::clone(&applications)))
        }
    };
    let mut service = HalquenService::new_with_application_registry(
        executor,
        database,
        applications,
        options.allow_unsafe_agents,
    )
    .map_err(|error| DaemonError::Initialization(error.to_string()))?;

    let runtime_paths = RuntimePaths::discover()?;
    service.set_environment(
        data_paths.database.display().to_string(),
        runtime_paths.socket.display().to_string(),
    );
    let service = Arc::new(Mutex::new(service));
    let cancellations = Arc::new(Mutex::new(BTreeMap::new()));
    runtime_paths.prepare_server()?;
    let listener = UnixListener::bind(&runtime_paths.socket)?;
    runtime_paths.secure_bound_socket()?;
    let _socket_guard = SocketGuard::new(runtime_paths.socket.clone());
    tracing::info!(component = "daemon", "Halquen daemon is ready");
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            signal = &mut shutdown => {
                signal?;
                tracing::info!(component = "daemon", "Halquen daemon is stopping");
                break;
            }
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let service = Arc::clone(&service);
                let cancellations = Arc::clone(&cancellations);
                tokio::spawn(async move {
                    if let Err(error) = handle_connection(stream, service, cancellations).await {
                        tracing::warn!(
                            component = "ipc",
                            error_code = "request_rejected",
                            "Rejected a local IPC request: {}",
                            crate::logging::redact(&error.to_string())
                        );
                    }
                });
            }
        }
    }
    let mut service = service.lock().await;
    let daemon_session_id = service.daemon_session_id.clone();
    service
        .database
        .finish_daemon_session(&daemon_session_id, crate::service::now_ms())?;
    Ok(())
}

async fn handle_connection<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    service: SharedService,
    cancellations: CancellationRegistry,
) -> Result<(), DaemonError> {
    let frame = match timeout(IO_TIMEOUT, read_frame(&mut stream)).await {
        Ok(result) => result?,
        Err(_) => return Err(DaemonError::Timeout),
    };
    let response = match decode_request(&frame) {
        Ok(request) => dispatch_request(request, &service, &cancellations).await,
        Err(error) => codec_error_response(error),
    };
    let bytes = encode_response(&response)?;
    timeout(IO_TIMEOUT, stream.write_all(&bytes))
        .await
        .map_err(|_| DaemonError::Timeout)??;
    Ok(())
}

async fn dispatch_request(
    envelope: RequestEnvelope,
    service: &SharedService,
    cancellations: &CancellationRegistry,
) -> ResponseEnvelope {
    if let ProtocolRequest::CancelChat { request_id } = &envelope.request {
        let sender = cancellations.lock().await.remove(request_id);
        let requested = sender.is_some_and(|sender| sender.send(true).is_ok());
        return ResponseEnvelope {
            version: PROTOCOL_VERSION,
            request_id: envelope.request_id,
            response: ProtocolResponse::ChatCancellation { requested },
        };
    }

    if matches!(&envelope.request, ProtocolRequest::Chat { .. }) {
        let active_request_id = envelope.request_id.clone();
        let (sender, receiver) = watch::channel(false);
        {
            let mut pending = cancellations.lock().await;
            if pending.contains_key(&active_request_id) {
                return duplicate_request_response(active_request_id);
            }
            pending.insert(active_request_id.clone(), sender);
        }
        let response = service
            .lock()
            .await
            .handle_with_cancellation(envelope, Some(receiver))
            .await;
        cancellations.lock().await.remove(&active_request_id);
        return response;
    }

    service.lock().await.handle(envelope).await
}

fn duplicate_request_response(request_id: String) -> ResponseEnvelope {
    ResponseEnvelope {
        version: PROTOCOL_VERSION,
        request_id,
        response: ProtocolResponse::Error {
            error: ProtocolErrorBody {
                code: ProtocolErrorCode::MalformedRequest,
                message: "request identifier is already active".to_owned(),
            },
        },
    }
}

async fn read_frame<R: AsyncRead + Unpin>(stream: &mut R) -> Result<Vec<u8>, DaemonError> {
    let mut frame = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Err(DaemonError::IncompleteFrame);
        }
        if let Some(newline) = chunk[..read].iter().position(|byte| *byte == b'\n') {
            let used = newline + 1;
            if frame.len() + used > MAX_FRAME_SIZE {
                return Err(DaemonError::FrameTooLarge);
            }
            frame.extend_from_slice(&chunk[..used]);
            if used != read {
                return Err(DaemonError::TrailingBytes);
            }
            return Ok(frame);
        }
        if frame.len() + read > MAX_FRAME_SIZE {
            return Err(DaemonError::FrameTooLarge);
        }
        frame.extend_from_slice(&chunk[..read]);
    }
}

#[cfg(test)]
// The transport helpers below the tests are intentionally kept next to their production callers.
#[allow(clippy::items_after_test_module)]
mod tests {
    use tokio::io::{AsyncWriteExt, duplex};

    use super::*;

    #[tokio::test]
    async fn accepts_a_frame_at_the_exact_limit() {
        let (mut writer, mut reader) = duplex(MAX_FRAME_SIZE + 1);
        let mut bytes = vec![b'x'; MAX_FRAME_SIZE];
        bytes[MAX_FRAME_SIZE - 1] = b'\n';
        writer.write_all(&bytes).await.unwrap();
        assert_eq!(read_frame(&mut reader).await.unwrap().len(), MAX_FRAME_SIZE);
    }

    #[tokio::test]
    async fn rejects_a_frame_one_byte_over_the_limit() {
        let (mut writer, mut reader) = duplex(MAX_FRAME_SIZE + 2);
        let mut bytes = vec![b'x'; MAX_FRAME_SIZE + 1];
        bytes[MAX_FRAME_SIZE] = b'\n';
        writer.write_all(&bytes).await.unwrap();
        assert!(matches!(
            read_frame(&mut reader).await,
            Err(DaemonError::FrameTooLarge)
        ));
    }

    #[tokio::test]
    async fn accepts_partial_reads_until_newline() {
        let (mut writer, mut reader) = duplex(64);
        writer.write_all(b"{\"part\":").await.unwrap();
        writer.write_all(b"true}\n").await.unwrap();
        assert_eq!(read_frame(&mut reader).await.unwrap(), b"{\"part\":true}\n");
    }

    #[tokio::test]
    async fn rejects_disconnect_without_newline() {
        let (mut writer, mut reader) = duplex(64);
        writer.write_all(b"{\"incomplete\":true}").await.unwrap();
        writer.shutdown().await.unwrap();
        assert!(matches!(
            read_frame(&mut reader).await,
            Err(DaemonError::IncompleteFrame)
        ));
    }

    #[tokio::test]
    async fn rejects_bytes_after_first_frame() {
        let (mut writer, mut reader) = duplex(64);
        writer.write_all(b"{}\n{}\n").await.unwrap();
        assert!(matches!(
            read_frame(&mut reader).await,
            Err(DaemonError::TrailingBytes)
        ));
    }

    #[tokio::test]
    async fn handles_sequential_connections_and_protocol_errors() {
        let service = Arc::new(Mutex::new(
            HalquenService::new(
                RuntimeExecutor::DryRun(DryRunExecutor::new()),
                Database::open_in_memory().unwrap(),
            )
            .unwrap(),
        ));
        let cancellations = Arc::new(Mutex::new(BTreeMap::new()));

        for request_id in ["request:first", "request:second"] {
            let (mut client, server) = duplex(MAX_FRAME_SIZE * 2);
            let request = halquen_protocol::RequestEnvelope {
                version: PROTOCOL_VERSION,
                request_id: request_id.to_owned(),
                request: halquen_protocol::ProtocolRequest::Health,
            };
            client
                .write_all(&halquen_protocol::encode_request(&request).unwrap())
                .await
                .unwrap();
            client.shutdown().await.unwrap();
            handle_connection(server, Arc::clone(&service), Arc::clone(&cancellations))
                .await
                .unwrap();
            let response =
                halquen_protocol::decode_response(&read_frame(&mut client).await.unwrap()).unwrap();
            assert_eq!(response.request_id, request_id);
        }

        for raw in [
            b"{not-json}\n".as_slice(),
            b"{\"version\":99,\"request_id\":\"request:old\",\"request\":{\"command\":\"health\"}}\n"
                .as_slice(),
        ] {
            let (mut client, server) = duplex(MAX_FRAME_SIZE * 2);
            client.write_all(raw).await.unwrap();
            client.shutdown().await.unwrap();
            handle_connection(
                server,
                Arc::clone(&service),
                Arc::clone(&cancellations),
            )
            .await
            .unwrap();
            let response = halquen_protocol::decode_response(
                &read_frame(&mut client).await.unwrap(),
            )
            .unwrap();
            assert!(matches!(response.response, ProtocolResponse::Error { .. }));
        }
    }

    #[tokio::test]
    async fn cancellation_bypasses_the_busy_service_lock() {
        let service = Arc::new(Mutex::new(
            HalquenService::new(
                RuntimeExecutor::DryRun(DryRunExecutor::new()),
                Database::open_in_memory().unwrap(),
            )
            .unwrap(),
        ));
        let cancellations = Arc::new(Mutex::new(BTreeMap::new()));
        let (sender, mut receiver) = watch::channel(false);
        cancellations
            .lock()
            .await
            .insert("request:active-chat".to_owned(), sender);

        let (mut client, server) = duplex(MAX_FRAME_SIZE * 2);
        let request = RequestEnvelope {
            version: PROTOCOL_VERSION,
            request_id: "request:cancel".to_owned(),
            request: ProtocolRequest::CancelChat {
                request_id: "request:active-chat".to_owned(),
            },
        };
        client
            .write_all(&halquen_protocol::encode_request(&request).unwrap())
            .await
            .unwrap();
        client.shutdown().await.unwrap();

        let _busy_service = service.lock().await;
        timeout(
            Duration::from_secs(1),
            handle_connection(server, Arc::clone(&service), Arc::clone(&cancellations)),
        )
        .await
        .unwrap()
        .unwrap();
        receiver.changed().await.unwrap();
        assert!(*receiver.borrow());
        let response =
            halquen_protocol::decode_response(&read_frame(&mut client).await.unwrap()).unwrap();
        assert!(matches!(
            response.response,
            ProtocolResponse::ChatCancellation { requested: true }
        ));
    }

    #[test]
    fn socket_guard_removes_only_a_known_socket() {
        let path = std::env::temp_dir().join(format!(
            "halquen-socket-guard-test-{}.sock",
            std::process::id()
        ));
        let listener = match std::os::unix::net::UnixListener::bind(&path) {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
            Err(error) => panic!("failed to bind test socket: {error}"),
        };
        let guard = SocketGuard::new(path.clone());
        drop(listener);
        drop(guard);
        assert!(!path.exists());
    }
}

fn codec_error_response(error: CodecError) -> ResponseEnvelope {
    let (code, message) = match error {
        CodecError::FrameTooLarge => (
            ProtocolErrorCode::FrameTooLarge,
            "request frame is too large".to_owned(),
        ),
        CodecError::UnknownVersion(_) => (
            ProtocolErrorCode::UnknownVersion,
            "unsupported protocol version".to_owned(),
        ),
        _ => (
            ProtocolErrorCode::MalformedRequest,
            "malformed request".to_owned(),
        ),
    };
    ResponseEnvelope {
        version: PROTOCOL_VERSION,
        request_id: "request:invalid".to_owned(),
        response: ProtocolResponse::Error {
            error: ProtocolErrorBody { code, message },
        },
    }
}

struct SocketGuard {
    path: PathBuf,
}

impl SocketGuard {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        remove_known_socket(&self.path);
    }
}

fn remove_known_socket(path: &Path) {
    if let Ok(metadata) = fs::symlink_metadata(path)
        && metadata.file_type().is_socket()
    {
        let _ = fs::remove_file(path);
    }
}
