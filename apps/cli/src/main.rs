#![forbid(unsafe_code)]

use std::env;
use std::error::Error;
use std::time::Duration;

use halquen_domain::{ActionArguments, ActionRequest, CapabilityId, EntityId};
use halquen_protocol::{
    MAX_FRAME_SIZE, PROTOCOL_VERSION, ProtocolRequest, ProtocolResponse, RequestEnvelope,
    RuntimePaths, decode_response, encode_request, request_id,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::timeout;

const IO_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("halquen: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn Error>> {
    let request = parse_command(env::args().skip(1).collect())?;
    let request_id = request_id();
    let envelope = RequestEnvelope {
        version: PROTOCOL_VERSION,
        request_id: request_id.clone(),
        request,
    };
    let paths = RuntimePaths::discover()?;
    paths.validate_client()?;
    let mut stream = UnixStream::connect(&paths.socket).await?;
    let frame = encode_request(&envelope)?;
    timeout(IO_TIMEOUT, stream.write_all(&frame)).await??;
    stream.shutdown().await?;
    let response_frame = timeout(IO_TIMEOUT, read_frame(&mut stream)).await??;
    let response = decode_response(&response_frame)?;
    if response.request_id != request_id && response.request_id != "request:invalid" {
        return Err("response request ID does not match".into());
    }
    print_response(response.response)?;
    Ok(())
}

fn parse_command(arguments: Vec<String>) -> Result<ProtocolRequest, Box<dyn Error>> {
    match arguments.as_slice() {
        [command] if command == "health" => Ok(ProtocolRequest::Health),
        [group, command] if group == "capabilities" && command == "list" => {
            Ok(ProtocolRequest::ListCapabilities)
        }
        [group, command, id] if group == "capability" && command == "get" => {
            Ok(ProtocolRequest::GetCapability {
                capability_id: CapabilityId::new(id)?,
            })
        }
        [command, operation, entity] if command == "dry-run" && operation == "open-app" => {
            Ok(ProtocolRequest::DryRunAction {
                action: open_app_action(entity)?,
            })
        }
        [command, operation, entity] if command == "evaluate" && operation == "open-app" => {
            Ok(ProtocolRequest::EvaluateAction {
                action: open_app_action(entity)?,
            })
        }
        [group, command] if group == "memory" && command == "stats" => {
            Ok(ProtocolRequest::MemoryStats)
        }
        [group, command] if group == "audit" && command == "stats" => {
            Ok(ProtocolRequest::AuditStats)
        }
        [command] if command == "--help" || command == "-h" => Err(usage().into()),
        [] => Err(usage().into()),
        _ => Err(format!("unsupported command\n\n{}", usage()).into()),
    }
}

fn open_app_action(entity: &str) -> Result<ActionRequest, Box<dyn Error>> {
    Ok(ActionRequest::new(
        CapabilityId::new("system.open_app")?,
        ActionArguments::OpenApp {
            app: EntityId::new(entity)?,
        },
    ))
}

fn usage() -> &'static str {
    "Usage:\n  halquen health\n  halquen capabilities list\n  halquen capability get <namespace.operation>\n  halquen evaluate open-app <app:entity>\n  halquen dry-run open-app <app:entity>\n  halquen memory stats\n  halquen audit stats"
}

async fn read_frame<R: AsyncRead + Unpin>(stream: &mut R) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut frame = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Err("daemon disconnected before terminating the response frame".into());
        }
        if let Some(newline) = chunk[..read].iter().position(|byte| *byte == b'\n') {
            let used = newline + 1;
            if frame.len() + used > MAX_FRAME_SIZE {
                return Err("response frame is too large".into());
            }
            frame.extend_from_slice(&chunk[..used]);
            if used != read {
                return Err("response contains trailing bytes".into());
            }
            return Ok(frame);
        }
        if frame.len() + read > MAX_FRAME_SIZE {
            return Err("response frame is too large".into());
        }
        frame.extend_from_slice(&chunk[..read]);
    }
}

fn print_response(response: ProtocolResponse) -> Result<(), Box<dyn Error>> {
    match response {
        ProtocolResponse::Health {
            status,
            schema_version,
        } => println!("status={status:?} schema_version={schema_version}"),
        ProtocolResponse::Capabilities { capabilities } => {
            for capability in capabilities {
                println!("{} v{} {:?}", capability.id, capability.version, capability.risk);
            }
        }
        ProtocolResponse::Capability { capability: Some(capability) } => println!(
            "{} v{} {:?}: {}",
            capability.id, capability.version, capability.risk, capability.description
        ),
        ProtocolResponse::Capability { capability: None } => println!("capability not found"),
        ProtocolResponse::Evaluation { decision } => {
            println!("decision={:?} reason={:?}", decision.outcome, decision.reason)
        }
        ProtocolResponse::DryRun { decision, receipt } => println!(
            "decision={:?} status={:?} execution_id={}",
            decision.outcome, receipt.status, receipt.execution_id
        ),
        ProtocolResponse::MemoryStats {
            items,
            revisions,
            evidence,
            unknown_cases,
        } => println!(
            "items={items} revisions={revisions} evidence={evidence} unknown_cases={unknown_cases}"
        ),
        ProtocolResponse::AuditStats {
            records,
            executions,
        } => println!("records={records} executions={executions}"),
        ProtocolResponse::Error { error } => {
            return Err(format!("{:?}: {}", error.code, error.message).into());
        }
    }
    Ok(())
}
