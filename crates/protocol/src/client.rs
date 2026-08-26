use std::time::Duration;

use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::timeout;

use crate::{
    CodecError, MAX_FRAME_SIZE, PROTOCOL_VERSION, ProtocolRequest, ProtocolResponse,
    RequestEnvelope, RuntimePathError, RuntimePaths, decode_response, encode_request, request_id,
};

const IO_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("runtime path is unavailable: {0}")]
    Runtime(#[from] RuntimePathError),
    #[error("daemon IPC failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("protocol encoding failed: {0}")]
    Protocol(#[from] CodecError),
    #[error("daemon IPC timed out")]
    Timeout,
    #[error("daemon disconnected before completing a response")]
    IncompleteFrame,
    #[error("daemon response is too large")]
    FrameTooLarge,
    #[error("daemon returned trailing response bytes")]
    TrailingBytes,
    #[error("daemon response identifier did not match the request")]
    RequestIdMismatch,
}

#[derive(Debug, Clone)]
pub struct DaemonClient {
    paths: RuntimePaths,
}

impl DaemonClient {
    pub fn discover() -> Result<Self, ClientError> {
        Ok(Self {
            paths: RuntimePaths::discover()?,
        })
    }

    pub fn with_paths(paths: RuntimePaths) -> Self {
        Self { paths }
    }

    pub async fn request(&self, request: ProtocolRequest) -> Result<ProtocolResponse, ClientError> {
        self.request_with_id(request_id(), request).await
    }

    pub async fn request_with_id(
        &self,
        request_id: String,
        request: ProtocolRequest,
    ) -> Result<ProtocolResponse, ClientError> {
        self.paths.validate_client()?;
        let envelope = RequestEnvelope {
            version: PROTOCOL_VERSION,
            request_id: request_id.clone(),
            request,
        };
        let mut stream = timeout(IO_TIMEOUT, UnixStream::connect(&self.paths.socket))
            .await
            .map_err(|_| ClientError::Timeout)??;
        let frame = encode_request(&envelope)?;
        timeout(IO_TIMEOUT, stream.write_all(&frame))
            .await
            .map_err(|_| ClientError::Timeout)??;
        stream.shutdown().await?;
        let response_frame = timeout(IO_TIMEOUT, read_frame(&mut stream))
            .await
            .map_err(|_| ClientError::Timeout)??;
        let response = decode_response(&response_frame)?;
        if response.request_id != request_id && response.request_id != "request:invalid" {
            return Err(ClientError::RequestIdMismatch);
        }
        Ok(response.response)
    }
}

async fn read_frame<R: AsyncRead + Unpin>(stream: &mut R) -> Result<Vec<u8>, ClientError> {
    let mut frame = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Err(ClientError::IncompleteFrame);
        }
        if let Some(newline) = chunk[..read].iter().position(|byte| *byte == b'\n') {
            let used = newline + 1;
            if frame.len() + used > MAX_FRAME_SIZE {
                return Err(ClientError::FrameTooLarge);
            }
            frame.extend_from_slice(&chunk[..used]);
            if used != read {
                return Err(ClientError::TrailingBytes);
            }
            return Ok(frame);
        }
        if frame.len() + read > MAX_FRAME_SIZE {
            return Err(ClientError::FrameTooLarge);
        }
        frame.extend_from_slice(&chunk[..read]);
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncWriteExt, duplex};

    use super::*;

    #[tokio::test]
    async fn response_reader_rejects_disconnect_and_trailing_frames() {
        let (mut writer, mut reader) = duplex(64);
        writer.write_all(b"{}\n{}\n").await.unwrap();
        assert!(matches!(
            read_frame(&mut reader).await,
            Err(ClientError::TrailingBytes)
        ));

        let (mut writer, mut reader) = duplex(64);
        writer.write_all(b"{}").await.unwrap();
        writer.shutdown().await.unwrap();
        assert!(matches!(
            read_frame(&mut reader).await,
            Err(ClientError::IncompleteFrame)
        ));
    }
}
