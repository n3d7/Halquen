#![forbid(unsafe_code)]

use std::env;
use std::error::Error;

use halquen_domain::{
    ActionArguments, ActionRequest, AgentId, CapabilityId, EntityId, ExecutableOwnership,
    ModelSelection,
};
use halquen_protocol::{
    AgentRunRequest, ApplicationRegistrationUpsert, ChatRequest, DaemonClient, ProtocolRequest,
    ProtocolResponse,
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
        [group, command, id] if group == "capabilities" && command == "describe" => {
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
        [command, operation, entity] if command == "execute" && operation == "open-app" => {
            Ok(ProtocolRequest::ExecuteAction {
                action: open_app_action(entity)?,
            })
        }
        [group, command] if group == "agents" && command == "list" => {
            Ok(ProtocolRequest::ListAgents { limit: 100 })
        }
        [group, command] if group == "agents" && command == "sessions" => {
            Ok(ProtocolRequest::ListAgentSessions { limit: 100 })
        }
        [group, command, agent_id, input @ ..]
            if group == "agents" && command == "run" && !input.is_empty() =>
        {
            Ok(ProtocolRequest::RunAgent {
                request: AgentRunRequest {
                    agent_id: AgentId::new(agent_id)?,
                    input: input.join(" "),
                },
            })
        }
        [group, command] if group == "security" && command == "permissions" => {
            Ok(ProtocolRequest::ListPermissionGrants { limit: 200 })
        }
        [group, command] if group == "applications" && command == "list" => {
            Ok(ProtocolRequest::ListRegisteredApplications { limit: 200 })
        }
        [group, command, entity, display_name, executable]
            if group == "applications" && command == "register" =>
        {
            Ok(ProtocolRequest::UpsertRegisteredApplication {
                application: ApplicationRegistrationUpsert {
                    entity_id: EntityId::new(entity)?,
                    display_name: display_name.clone(),
                    executable: executable.clone(),
                    arguments: Vec::new(),
                    ownership: ExecutableOwnership::RootOrCurrentUser,
                    sha256_hex: None,
                    enabled: true,
                },
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
    "Usage:\n  halquen health\n  halquen capabilities list\n  halquen capabilities describe <namespace.operation>\n  halquen evaluate open-app <app:entity>\n  halquen dry-run open-app <app:entity>\n  halquen execute open-app <app:entity>\n  halquen agents list\n  halquen agents run <agent-id> <input>\n  halquen agents sessions\n  halquen security permissions\n  halquen applications list\n  halquen applications register <app:entity> <display-name> <absolute-executable>\n  halquen chat <message>\n  halquen memory stats\n  halquen audit stats"
}

fn print_response(response: ProtocolResponse) -> Result<(), Box<dyn Error>> {
    match response {
        ProtocolResponse::Health {
            status,
            schema_version,
        } => println!("status={status:?} schema_version={schema_version}"),
        ProtocolResponse::Capabilities { capabilities } => {
            for capability in capabilities {
                println!(
                    "{} v{} {:?}",
                    capability.id, capability.version, capability.risk
                );
            }
        }
        ProtocolResponse::Capability {
            capability: Some(capability),
        } => println!(
            "{} v{} {:?}: {}",
            capability.id, capability.version, capability.risk, capability.description
        ),
        ProtocolResponse::Capability { capability: None } => println!("capability not found"),
        ProtocolResponse::Evaluation { decision } => {
            println!(
                "decision={:?} reason={:?}",
                decision.outcome, decision.reason
            )
        }
        ProtocolResponse::DryRun { decision, receipt } => println!(
            "decision={:?} status={:?} execution_id={}",
            decision.outcome, receipt.status, receipt.execution_id
        ),
        ProtocolResponse::Execution { decision, receipt } => println!(
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
        ProtocolResponse::ChatCancellation { requested } => {
            println!("cancellation_requested={requested}")
        }
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
        ProtocolResponse::ProviderTest { result } => {
            println!("status={:?} {}", result.status, result.message)
        }
        ProtocolResponse::Models { models } => println!("models={}", models.len()),
        ProtocolResponse::ModelSaved { model } => println!("model={}", model.display_name),
        ProtocolResponse::ApplicationSettings { .. } | ProtocolResponse::SettingsUpdated { .. } => {
            println!("settings=ok")
        }
        ProtocolResponse::UsageStats { stats } => println!(
            "local={} ai={} cache_hits={}",
            stats.local_resolutions, stats.ai_fallbacks, stats.response_cache_hits
        ),
        ProtocolResponse::Diagnostics { snapshot } => println!(
            "protocol={} schema={} diagnostics={}",
            snapshot.protocol_version,
            snapshot.schema_version,
            snapshot.recent.len()
        ),
        ProtocolResponse::OperationalLogsCleared { removed } => {
            println!("historical_logs_removed={removed}")
        }
        ProtocolResponse::FeedbackRecorded => println!("feedback=recorded"),
        ProtocolResponse::Confirmation { result } => {
            println!("accepted={} {}", result.accepted, result.message)
        }
        ProtocolResponse::AiRequestPreview { preview } => println!(
            "task={:?} context_tokens={} core_contract_managed={}",
            preview.task, preview.estimated_context_tokens, preview.core_contract_managed
        ),
        ProtocolResponse::SecurityOverview { overview } => println!(
            "security_profile={:?} permissions={} resource_labels={} agents={}",
            overview.profile,
            overview.active_permissions,
            overview.resource_labels,
            overview.configured_agents
        ),
        ProtocolResponse::SecurityProfileUpdated { profile } => {
            println!("security_profile={profile:?}")
        }
        ProtocolResponse::PermissionGrants { grants } => {
            for grant in grants {
                println!(
                    "{} {:?} {:?} capability={} agent={}",
                    grant.id,
                    grant.effect,
                    grant.lifetime,
                    grant.scope.capability_id,
                    grant.agent_id.as_ref().map_or("-", AgentId::as_str),
                );
            }
        }
        ProtocolResponse::PermissionSaved { grant } => println!("permission={}", grant.id),
        ProtocolResponse::PermissionRevoked { revoked } => println!("revoked={revoked}"),
        ProtocolResponse::ResourceLabels { labels } => {
            println!("resource_labels={}", labels.len())
        }
        ProtocolResponse::ResourceLabelSaved { label } => {
            println!("resource_label={}", label.name)
        }
        ProtocolResponse::ResourceLabelRemoved { removed } => println!("removed={removed}"),
        ProtocolResponse::Agents { agents } => println!("agents={}", agents.len()),
        ProtocolResponse::AgentSaved { agent } => println!("agent={}", agent.name),
        ProtocolResponse::AgentRemoved { removed } => println!("removed={removed}"),
        ProtocolResponse::AgentRun { result } => {
            println!(
                "session={} status={:?} proposals={} {}",
                result.session.id,
                result.session.status,
                result.proposals.len(),
                result.message
            );
            for proposal in result.proposals {
                println!(
                    "proposal={} capability={} status={:?} {}",
                    proposal.index, proposal.capability_id, proposal.disposition, proposal.message
                );
            }
        }
        ProtocolResponse::AgentSessions { sessions } => {
            for session in sessions {
                println!(
                    "{} agent={} instance={} status={:?}",
                    session.id, session.agent_id, session.instance_id, session.status
                );
            }
        }
        ProtocolResponse::RegisteredApplications { applications } => {
            for application in applications {
                println!(
                    "{} {} executable={} enabled={}",
                    application.entity_id,
                    application.display_name,
                    application.executable,
                    application.enabled
                );
            }
        }
        ProtocolResponse::RegisteredApplicationSaved { application } => {
            println!("application={}", application.entity_id)
        }
        ProtocolResponse::RegisteredApplicationRemoved { removed } => {
            println!("removed={removed}")
        }
        ProtocolResponse::Error { error } => {
            return Err(format!("{:?}: {}", error.code, error.message).into());
        }
    }
    Ok(())
}
