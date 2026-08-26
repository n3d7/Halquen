#![forbid(unsafe_code)]

use std::env;
use std::error::Error;

use halquen_domain::{ActionArguments, ActionRequest, CapabilityId, EntityId, ModelSelection};
use halquen_protocol::{
    ChatRequest, DaemonClient, ProtocolRequest, ProtocolResponse,
};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("halquen: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn Error>> {
    let request = parse_command(env::args().skip(1).collect())?;
    let response = DaemonClient::discover()?.request(request).await?;
    print_response(response)?;
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
        [command, message @ ..] if command == "chat" && !message.is_empty() => {
            Ok(ProtocolRequest::Chat {
                request: ChatRequest {
                    session_id: None,
                    message: message.join(" "),
                    model_selection: ModelSelection::Automatic,
                },
            })
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
    "Usage:\n  halquen health\n  halquen capabilities list\n  halquen capability get <namespace.operation>\n  halquen evaluate open-app <app:entity>\n  halquen dry-run open-app <app:entity>\n  halquen chat <message>\n  halquen memory stats\n  halquen audit stats"
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
        ProtocolResponse::Chat { result } => println!("{}", result.assistant_message.content),
        ProtocolResponse::ChatSessions { sessions } => println!("sessions={}", sessions.len()),
        ProtocolResponse::ChatMessages { messages } => println!("messages={}", messages.len()),
        ProtocolResponse::Activity { events } => println!("activity_events={}", events.len()),
        ProtocolResponse::MemoryItems { items } => println!("memory_items={}", items.len()),
        ProtocolResponse::MemoryHistory { revisions } => println!("revisions={}", revisions.len()),
        ProtocolResponse::MemoryUpdated { updated } => println!("updated={updated}"),
        ProtocolResponse::MemoryMutation { receipt } => println!(
            "memory_id={} revision_id={} {}",
            receipt.memory_id, receipt.revision_id, receipt.summary
        ),
        ProtocolResponse::Providers { providers } => println!("providers={}", providers.len()),
        ProtocolResponse::ProviderSaved { provider } => println!("provider={}", provider.name),
        ProtocolResponse::ProviderRemoved { removed } => println!("removed={removed}"),
        ProtocolResponse::ProviderTest { result } => println!("status={:?} {}", result.status, result.message),
        ProtocolResponse::Models { models } => println!("models={}", models.len()),
        ProtocolResponse::ModelSaved { model } => println!("model={}", model.display_name),
        ProtocolResponse::ApplicationSettings { .. }
        | ProtocolResponse::SettingsUpdated { .. } => println!("settings=ok"),
        ProtocolResponse::UsageStats { stats } => println!(
            "local={} ai={} cache_hits={}",
            stats.local_resolutions, stats.ai_fallbacks, stats.response_cache_hits
        ),
        ProtocolResponse::Diagnostics { snapshot } => println!(
            "protocol={} schema={} diagnostics={}",
            snapshot.protocol_version, snapshot.schema_version, snapshot.recent.len()
        ),
        ProtocolResponse::FeedbackRecorded => println!("feedback=recorded"),
        ProtocolResponse::Confirmation { result } => println!("accepted={} {}", result.accepted, result.message),
        ProtocolResponse::AiRequestPreview { preview } => println!(
            "task={:?} context_tokens={} core_contract_managed={}",
            preview.task, preview.estimated_context_tokens, preview.core_contract_managed
        ),
        ProtocolResponse::Error { error } => {
            return Err(format!("{:?}: {}", error.code, error.message).into());
        }
    }
    Ok(())
}
