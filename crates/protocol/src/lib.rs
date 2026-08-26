#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;

pub mod client;
pub mod interaction;
pub mod request;
pub mod response;
pub mod transport;

pub use client::{ClientError, DaemonClient};
pub use interaction::{
    ChatRequest, ChatResult, ConfirmationPrompt, ConfirmationResult, DiagnosticsSnapshot,
    MemoryMutationReceipt, MemoryQuery, MemoryStateUpdate, ModelUpsert, PromptPreview,
    ProviderTestStatus, ProviderUpsert,
};
pub use request::{ProtocolRequest, RequestEnvelope};
pub use response::{
    HealthStatus, ProtocolErrorBody, ProtocolErrorCode, ProtocolResponse, ResponseEnvelope,
};
pub use transport::{RuntimePathError, RuntimePaths};

pub const PROTOCOL_VERSION: u16 = 3;
pub const MAX_FRAME_SIZE: usize = 64 * 1024;

#[derive(Debug, Error)]
pub enum CodecError {
    #[error("frame exceeds {MAX_FRAME_SIZE} bytes")]
    FrameTooLarge,
    #[error("frame is empty")]
    EmptyFrame,
    #[error("frame contains more than one message")]
    MultipleMessages,
    #[error("unsupported protocol version {0}")]
    UnknownVersion(u16),
    #[error("invalid request identifier")]
    InvalidRequestId,
    #[error("malformed JSON frame: {0}")]
    Malformed(#[from] serde_json::Error),
}

pub fn request_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("request:{}:{sequence}", std::process::id())
}

pub fn encode_request(envelope: &RequestEnvelope) -> Result<Vec<u8>, CodecError> {
    validate_envelope(envelope.version, &envelope.request_id)?;
    encode(envelope)
}

pub fn decode_request(frame: &[u8]) -> Result<RequestEnvelope, CodecError> {
    let payload = frame_payload(frame)?;
    let envelope: RequestEnvelope = serde_json::from_slice(payload)?;
    validate_envelope(envelope.version, &envelope.request_id)?;
    Ok(envelope)
}

pub fn encode_response(envelope: &ResponseEnvelope) -> Result<Vec<u8>, CodecError> {
    validate_envelope(envelope.version, &envelope.request_id)?;
    encode(envelope)
}

pub fn decode_response(frame: &[u8]) -> Result<ResponseEnvelope, CodecError> {
    let payload = frame_payload(frame)?;
    let envelope: ResponseEnvelope = serde_json::from_slice(payload)?;
    validate_envelope(envelope.version, &envelope.request_id)?;
    Ok(envelope)
}

fn encode<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, CodecError> {
    let mut bytes = serde_json::to_vec(value)?;
    if bytes.len() + 1 > MAX_FRAME_SIZE {
        return Err(CodecError::FrameTooLarge);
    }
    bytes.push(b'\n');
    Ok(bytes)
}

fn frame_payload(frame: &[u8]) -> Result<&[u8], CodecError> {
    if frame.len() > MAX_FRAME_SIZE {
        return Err(CodecError::FrameTooLarge);
    }
    let payload = frame.strip_suffix(b"\n").unwrap_or(frame);
    if payload.is_empty() {
        return Err(CodecError::EmptyFrame);
    }
    if payload.contains(&b'\n') || payload.contains(&b'\r') {
        return Err(CodecError::MultipleMessages);
    }
    Ok(payload)
}

fn validate_envelope(version: u16, request_id: &str) -> Result<(), CodecError> {
    if version != PROTOCOL_VERSION {
        return Err(CodecError::UnknownVersion(version));
    }
    if request_id.is_empty()
        || request_id.len() > 128
        || !request_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'.' | b'_' | b'-'))
    {
        return Err(CodecError::InvalidRequestId);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn health() -> RequestEnvelope {
        RequestEnvelope {
            version: PROTOCOL_VERSION,
            request_id: "request:test".to_owned(),
            request: ProtocolRequest::Health,
        }
    }

    #[test]
    fn request_round_trip() {
        let request = health();
        let bytes = encode_request(&request).unwrap();
        assert_eq!(decode_request(&bytes).unwrap(), request);
    }

    #[test]
    fn unknown_version_fails_cleanly() {
        let mut request = health();
        request.version = 99;
        assert!(matches!(
            encode_request(&request),
            Err(CodecError::UnknownVersion(99))
        ));
    }

    #[test]
    fn malformed_request_is_rejected() {
        assert!(matches!(
            decode_request(b"{not-json}\n"),
            Err(CodecError::Malformed(_))
        ));
    }

    #[test]
    fn oversized_request_is_rejected_before_parsing() {
        let frame = vec![b'x'; MAX_FRAME_SIZE + 1];
        assert!(matches!(
            decode_request(&frame),
            Err(CodecError::FrameTooLarge)
        ));
    }

    #[test]
    fn multiple_messages_are_rejected() {
        assert!(matches!(
            decode_request(b"{}\n{}\n"),
            Err(CodecError::MultipleMessages)
        ));
    }

    #[test]
    fn response_round_trip() {
        let response = ResponseEnvelope {
            version: PROTOCOL_VERSION,
            request_id: "request:test".to_owned(),
            response: ProtocolResponse::Health {
                status: HealthStatus::Ok,
                schema_version: 1,
            },
        };
        let bytes = encode_response(&response).unwrap();
        assert_eq!(decode_response(&bytes).unwrap(), response);
    }

    #[test]
    fn gui_chat_request_round_trip_preserves_manual_model_selection() {
        let model_id = halquen_domain::ModelId::generate();
        let request = RequestEnvelope {
            version: PROTOCOL_VERSION,
            request_id: "request:gui-chat".to_owned(),
            request: ProtocolRequest::Chat {
                request: ChatRequest {
                    session_id: None,
                    message: "Explain the local route".to_owned(),
                    model_selection: halquen_domain::ModelSelection::Model {
                        model_id: model_id.clone(),
                    },
                },
            },
        };
        let decoded = decode_request(&encode_request(&request).unwrap()).unwrap();
        assert_eq!(decoded, request);
        assert!(matches!(
            decoded.request,
            ProtocolRequest::Chat {
                request: ChatRequest {
                    model_selection: halquen_domain::ModelSelection::Model { model_id: id },
                    ..
                }
            } if id == model_id
        ));
    }

    #[test]
    fn chat_cancellation_round_trip_preserves_target_request_id() {
        let request = RequestEnvelope {
            version: PROTOCOL_VERSION,
            request_id: "request:cancel-command".to_owned(),
            request: ProtocolRequest::CancelChat {
                request_id: "request:chat-target".to_owned(),
            },
        };
        let decoded = decode_request(&encode_request(&request).unwrap()).unwrap();
        assert_eq!(decoded, request);
    }
}
