use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use halquen_capabilities::DryRunExecutor;
use halquen_protocol::{
    CodecError, MAX_FRAME_SIZE, PROTOCOL_VERSION, ProtocolErrorBody, ProtocolErrorCode,
    ProtocolResponse, ResponseEnvelope, RuntimePaths, decode_request,
    encode_response,
};
use halquen_storage::{DataPaths, Database};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::time::timeout;

use crate::service::HalquenService;

const IO_TIMEOUT: Duration = Duration::from_secs(5);

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
}

pub async fn run() -> Result<(), DaemonError> {
    let data_paths = DataPaths::discover()?;
    data_paths.prepare()?;
    let database = Database::open(&data_paths.database)?;
    let mut service = HalquenService::new(DryRunExecutor::new(), database)
        .map_err(|error| DaemonError::Initialization(error.to_string()))?;

    let runtime_paths = RuntimePaths::discover()?;
    runtime_paths.prepare_server()?;
    let listener = UnixListener::bind(&runtime_paths.socket)?;
    runtime_paths.secure_bound_socket()?;
    let _socket_guard = SocketGuard::new(runtime_paths.socket.clone());
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            signal = &mut shutdown => {
                signal?;
                break;
            }
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                if let Err(error) = handle_connection(stream, &mut service).await {
                    eprintln!("halquen-daemon: rejected local request: {error}");
                }
            }
        }
    }
    Ok(())
}

async fn handle_connection<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    service: &mut HalquenService<DryRunExecutor>,
) -> Result<(), DaemonError> {
    let frame = match timeout(IO_TIMEOUT, read_frame(&mut stream)).await {
        Ok(result) => result?,
        Err(_) => return Err(DaemonError::Timeout),
    };
    let response = match decode_request(&frame) {
        Ok(request) => service.handle(request).await,
        Err(error) => codec_error_response(error),
    };
    let bytes = encode_response(&response)?;
    timeout(IO_TIMEOUT, stream.write_all(&bytes))
        .await
        .map_err(|_| DaemonError::Timeout)??;
    Ok(())
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
        assert_eq!(
            read_frame(&mut reader).await.unwrap(),
            b"{\"part\":true}\n"
        );
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
        let mut service = HalquenService::new(
            DryRunExecutor::new(),
            Database::open_in_memory().unwrap(),
        )
        .unwrap();

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
            handle_connection(server, &mut service).await.unwrap();
            let response = halquen_protocol::decode_response(
                &read_frame(&mut client).await.unwrap(),
            )
            .unwrap();
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
            handle_connection(server, &mut service).await.unwrap();
            let response = halquen_protocol::decode_response(
                &read_frame(&mut client).await.unwrap(),
            )
            .unwrap();
            assert!(matches!(response.response, ProtocolResponse::Error { .. }));
        }
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
