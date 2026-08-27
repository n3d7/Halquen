use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use halquen_ai::{
    AgentHost, DisabledProviderClient, KeyringSecretStore, OpenAiCompatibleClient, ProviderClient,
    SecretStore,
};
use halquen_audit::{AuditEvent, AuditRecord, ExecutionReceipt, ExecutionStatus, SafeResultCode};
use halquen_capabilities::{
    ApplicationRegistry, CapabilityRegistry, DryRunExecutor, ExecutionError, ExecutionResultCode,
    Executor, SharedApplicationRegistry, open_app_descriptor,
};
use halquen_domain::{
    ActionArguments, ActionContext, ActionProposal, ActionRequest, AuditId, CapabilityDescriptor,
    DaemonSession, DaemonSessionId, DiagnosticEntry, ExecutionId, PermissionSessionScope,
    ProposalId, ResourceClassification, ResourceDescriptor, ResourceKind,
};
use halquen_policy::{PolicyContext, PolicyEngine, PolicyOutcome, PolicyReason};
use halquen_protocol::{
    HealthStatus, PROTOCOL_VERSION, ProtocolErrorBody, ProtocolErrorCode, ProtocolRequest,
    ProtocolResponse, RequestEnvelope, ResponseEnvelope,
};
use halquen_storage::Database;
use tokio::sync::watch;
use tokio::time::timeout;

pub struct HalquenService<E> {
    pub(crate) registry: CapabilityRegistry,
    pub(crate) policy: PolicyEngine,
    pub(crate) executor: E,
    pub(crate) dry_run_executor: DryRunExecutor,
    pub(crate) database: Database,
    pub(crate) applications: SharedApplicationRegistry,
    pub(crate) agent_host: AgentHost,
    pub(crate) daemon_session_id: DaemonSessionId,
    pub(crate) provider_client: Arc<dyn ProviderClient>,
    pub(crate) secret_store: Arc<dyn SecretStore>,
    pub(crate) pending_confirmations: BTreeMap<String, PendingConfirmation>,
    pub(crate) diagnostics: VecDeque<DiagnosticEntry>,
    pub(crate) environment: ServiceEnvironment,
}

pub(crate) struct PendingConfirmation {
    pub request_execution_id: ExecutionId,
    pub proposal: ActionProposal,
    pub title: String,
    pub expires_at_ms: i64,
    pub session_id: Option<halquen_domain::ChatSessionId>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ServiceEnvironment {
    pub database_path: String,
    pub runtime_socket: String,
}

impl<E: Executor> HalquenService<E> {
    pub fn new(executor: E, database: Database) -> Result<Self, Box<dyn std::error::Error>> {
        let applications = Arc::new(RwLock::new(ApplicationRegistry::from_applications(
            database.list_registered_applications(200)?,
        )?));
        Self::new_with_application_registry(executor, database, applications, false)
    }

    pub fn new_with_application_registry(
        executor: E,
        mut database: Database,
        applications: SharedApplicationRegistry,
        allow_unsafe_agents: bool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut registry = CapabilityRegistry::new();
        registry.register(open_app_descriptor())?;
        let daemon_session_id = DaemonSessionId::generate();
        database.begin_daemon_session(&DaemonSession {
            id: daemon_session_id.clone(),
            started_at_ms: now_ms(),
            ended_at_ms: None,
        })?;
        Ok(Self {
            registry,
            policy: PolicyEngine::new(),
            executor,
            dry_run_executor: DryRunExecutor::new(),
            database,
            applications,
            agent_host: if allow_unsafe_agents {
                AgentHost::with_unsafe_unsandboxed_opt_in()
            } else {
                AgentHost::new()
            },
            daemon_session_id,
            provider_client: Arc::new(OpenAiCompatibleClient::new()?),
            secret_store: Arc::new(KeyringSecretStore::new("halquen.ai-provider")),
            pending_confirmations: BTreeMap::new(),
            diagnostics: VecDeque::with_capacity(200),
            environment: ServiceEnvironment::default(),
        })
    }

    pub fn from_parts(
        registry: CapabilityRegistry,
        policy: PolicyEngine,
        executor: E,
        mut database: Database,
    ) -> Self {
        let applications = Arc::new(RwLock::new(
            ApplicationRegistry::from_applications(
                database
                    .list_registered_applications(200)
                    .unwrap_or_default(),
            )
            .unwrap_or_default(),
        ));
        let daemon_session_id = DaemonSessionId::generate();
        let _ = database.begin_daemon_session(&DaemonSession {
            id: daemon_session_id.clone(),
            started_at_ms: now_ms(),
            ended_at_ms: None,
        });
        Self {
            registry,
            policy,
            executor,
            dry_run_executor: DryRunExecutor::new(),
            database,
            applications,
            agent_host: AgentHost::new(),
            daemon_session_id,
            provider_client: Arc::new(DisabledProviderClient),
            secret_store: Arc::new(KeyringSecretStore::new("halquen.ai-provider")),
            pending_confirmations: BTreeMap::new(),
            diagnostics: VecDeque::with_capacity(200),
            environment: ServiceEnvironment::default(),
        }
    }

    pub fn set_environment(&mut self, database_path: String, runtime_socket: String) {
        self.environment = ServiceEnvironment {
            database_path,
            runtime_socket,
        };
    }

    pub fn database(&self) -> &Database {
        &self.database
    }

    pub async fn handle(&mut self, envelope: RequestEnvelope) -> ResponseEnvelope {
        self.handle_with_cancellation(envelope, None).await
    }

    pub(crate) async fn handle_with_cancellation(
        &mut self,
        envelope: RequestEnvelope,
        cancellation: Option<watch::Receiver<bool>>,
    ) -> ResponseEnvelope {
        let request_id = envelope.request_id;
        let response = match envelope.request {
            ProtocolRequest::Health => self.health(),
            ProtocolRequest::ListCapabilities => Ok(ProtocolResponse::Capabilities {
                capabilities: self.registry.list().cloned().collect(),
            }),
            ProtocolRequest::GetCapability { capability_id } => Ok(ProtocolResponse::Capability {
                capability: self.registry.get(&capability_id).cloned(),
            }),
            ProtocolRequest::EvaluateAction { action } => self.evaluate_action(action),
            ProtocolRequest::DryRunAction { action } => self.dry_run_action(action).await,
            ProtocolRequest::ExecuteAction { action } => self.execute_action(action).await,
            ProtocolRequest::MemoryStats => self.memory_stats(),
            ProtocolRequest::AuditStats => self.audit_stats(),
            ProtocolRequest::Chat { request } => match cancellation {
                Some(signal) => {
                    self.chat_with_cancellation(request, &request_id, signal)
                        .await
                }
                None => self.chat(request, &request_id).await,
            },
            ProtocolRequest::CancelChat { .. } => {
                Ok(ProtocolResponse::ChatCancellation { requested: false })
            }
            ProtocolRequest::ListChatSessions { limit } => self.list_chat_sessions(limit),
            ProtocolRequest::ListChatMessages { session_id, limit } => {
                self.list_chat_messages(&session_id, limit)
            }
            ProtocolRequest::ListActivity { limit } => self.list_activity(limit),
            ProtocolRequest::ListMemory { query } => self.list_memory(query),
            ProtocolRequest::GetMemoryHistory { memory_id } => self.memory_history(&memory_id),
            ProtocolRequest::UpdateMemoryState { update } => self.update_memory_state(update),
            ProtocolRequest::RestoreMemoryRevision {
                memory_id,
                revision_id,
            } => self.restore_memory_revision(&memory_id, &revision_id),
            ProtocolRequest::ListProviders => self.list_providers(),
            ProtocolRequest::UpsertProvider { provider } => self.upsert_provider(provider),
            ProtocolRequest::RemoveProvider { provider_id } => self.remove_provider(&provider_id),
            ProtocolRequest::TestProvider { provider_id } => self.test_provider(&provider_id).await,
            ProtocolRequest::ListModels => self.list_models(),
            ProtocolRequest::UpsertModel { model } => self.upsert_model(model),
            ProtocolRequest::GetApplicationSettings => self.application_settings(),
            ProtocolRequest::UpdateApplicationSettings { settings } => {
                self.update_application_settings(settings)
            }
            ProtocolRequest::GetUsageStats => self.usage_stats(),
            ProtocolRequest::GetDiagnostics { limit } => self.diagnostics(limit),
            ProtocolRequest::ClearOperationalLogs => self.clear_operational_logs(),
            ProtocolRequest::SubmitResponseFeedback {
                cache_entry_id,
                feedback,
            } => self.submit_response_feedback(&cache_entry_id, feedback),
            ProtocolRequest::ConfirmAction {
                confirmation_id,
                allow,
                persistence,
                expires_at_ms,
            } => {
                self.confirm_action(&confirmation_id, allow, persistence, expires_at_ms)
                    .await
            }
            ProtocolRequest::PreviewAiRequest { request } => self.preview_ai_request(request),
            ProtocolRequest::GetSecurityOverview => self.security_overview(),
            ProtocolRequest::UpdateSecurityProfile { profile } => {
                self.update_security_profile(profile)
            }
            ProtocolRequest::ListPermissionGrants { limit } => self.list_permission_grants(limit),
            ProtocolRequest::UpsertPermissionGrant { grant } => self.upsert_permission_grant(grant),
            ProtocolRequest::RevokePermissionGrant { permission_id } => {
                self.revoke_permission_grant(&permission_id)
            }
            ProtocolRequest::ListResourceLabels { limit } => self.list_resource_labels(limit),
            ProtocolRequest::UpsertResourceLabel { label } => self.upsert_resource_label(label),
            ProtocolRequest::RemoveResourceLabel { resource_label_id } => {
                self.remove_resource_label(&resource_label_id)
            }
            ProtocolRequest::ListAgents { limit } => self.list_agents(limit),
            ProtocolRequest::UpsertAgent { agent } => self.upsert_agent(agent),
            ProtocolRequest::RemoveAgent { agent_id } => self.remove_agent(&agent_id),
            ProtocolRequest::RunAgent { request } => self.run_agent(request).await,
            ProtocolRequest::ListAgentSessions { limit } => self.list_agent_sessions(limit),
            ProtocolRequest::ListRegisteredApplications { limit } => {
                self.list_registered_applications(limit)
            }
            ProtocolRequest::UpsertRegisteredApplication { application } => {
                self.upsert_registered_application(application)
            }
            ProtocolRequest::RemoveRegisteredApplication { entity_id } => {
                self.remove_registered_application(&entity_id)
            }
        }
        .unwrap_or_else(|error| ProtocolResponse::Error { error });

        ResponseEnvelope {
            version: PROTOCOL_VERSION,
            request_id,
            response,
        }
    }

    fn health(&self) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let schema_version = self.database.schema_version().map_err(internal_error)?;
        Ok(ProtocolResponse::Health {
            status: HealthStatus::Ok,
            schema_version,
        })
    }

    fn memory_stats(&self) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let stats = self.database.memory_stats().map_err(internal_error)?;
        Ok(ProtocolResponse::MemoryStats {
            items: stats.items,
            revisions: stats.revisions,
            evidence: stats.evidence,
            unknown_cases: stats.unknown_cases,
        })
    }

    fn audit_stats(&self) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let stats = self.database.audit_stats().map_err(internal_error)?;
        Ok(ProtocolResponse::AuditStats {
            records: stats.records,
            executions: stats.executions,
        })
    }

    fn evaluate_action(
        &mut self,
        action: ActionRequest,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let descriptor = self.descriptor_for(&action)?.clone();
        let proposal = self.trusted_user_proposal(action)?;
        let context = self.policy_context(&proposal, None)?;
        let decision = self
            .policy
            .evaluate_proposal(&descriptor, &proposal, &context);
        let timestamp = now_ms();
        let audit = AuditRecord {
            id: AuditId::generate(),
            created_at_ms: timestamp,
            event: AuditEvent::PolicyEvaluated {
                execution_id: None,
                capability_id: descriptor.id,
                decision: decision.clone(),
            },
        };
        self.database.append_audit(&audit).map_err(internal_error)?;
        Ok(ProtocolResponse::Evaluation { decision })
    }

    pub(crate) async fn dry_run_action(
        &mut self,
        action: ActionRequest,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let proposal = self.trusted_user_proposal(action)?;
        self.dry_run_proposal(proposal, None).await
    }

    pub(crate) async fn execute_action(
        &mut self,
        action: ActionRequest,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let proposal = self.trusted_user_proposal(action)?;
        self.execute_proposal(proposal, None).await
    }

    fn trusted_user_proposal(
        &self,
        action: ActionRequest,
    ) -> Result<ActionProposal, ProtocolErrorBody> {
        let context = match &action.arguments {
            ActionArguments::None => ActionContext::trusted_user(None),
            ActionArguments::OpenApp { app } => {
                let identifier = app.as_str().to_owned();
                let classification = self
                    .database
                    .resource_label_for(ResourceKind::Application, &identifier)
                    .map_err(internal_error)?
                    .map_or(ResourceClassification::Local, |label| label.classification);
                ActionContext::trusted_user(None).with_resource(ResourceDescriptor {
                    kind: ResourceKind::Application,
                    identifier,
                    classification,
                })
            }
        };
        ActionProposal::new(action, context).map_err(|_| invalid_action_context())
    }

    pub(crate) async fn dry_run_proposal(
        &mut self,
        proposal: ActionProposal,
        session_id: Option<&halquen_domain::ChatSessionId>,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        self.process_proposal(proposal, session_id, true).await
    }

    pub(crate) async fn execute_proposal(
        &mut self,
        proposal: ActionProposal,
        session_id: Option<&halquen_domain::ChatSessionId>,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        self.process_proposal(proposal, session_id, false).await
    }

    async fn process_proposal(
        &mut self,
        proposal: ActionProposal,
        session_id: Option<&halquen_domain::ChatSessionId>,
        simulate: bool,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let descriptor = self.descriptor_for(&proposal.action)?.clone();
        let execution_id = ExecutionId::generate();
        let proposal_id = ProposalId::generate();
        let started_at_ms = now_ms();
        let context = self.policy_context(&proposal, session_id)?;
        let evaluation = self.policy.authorize_proposal(
            &descriptor,
            proposal.clone(),
            execution_id.clone(),
            &context,
        );
        let decision = evaluation.decision.clone();
        let mut audit_records = vec![
            AuditRecord {
                id: AuditId::generate(),
                created_at_ms: started_at_ms,
                event: AuditEvent::ProposalCreated {
                    execution_id: execution_id.clone(),
                    proposal_id,
                    capability_id: descriptor.id.clone(),
                    context: proposal.context.sanitized_summary(),
                },
            },
            AuditRecord {
                id: AuditId::generate(),
                created_at_ms: started_at_ms,
                event: AuditEvent::ActionRequested {
                    execution_id: execution_id.clone(),
                    capability_id: descriptor.id.clone(),
                    capability_version: descriptor.version,
                },
            },
            AuditRecord {
                id: AuditId::generate(),
                created_at_ms: started_at_ms,
                event: AuditEvent::PolicyEvaluated {
                    execution_id: Some(execution_id.clone()),
                    capability_id: descriptor.id.clone(),
                    decision: decision.clone(),
                },
            },
        ];

        let (status, result_code, error_code, sanitized_error) = if let Some(authorization) =
            evaluation.into_authorization()
        {
            if decision.reason == PolicyReason::PersistentExactAllow {
                let permission_session = proposal
                    .context
                    .agent
                    .as_ref()
                    .map(|identity| {
                        halquen_domain::PermissionSessionScope::Agent(identity.session_id.clone())
                    })
                    .or_else(|| {
                        session_id
                            .cloned()
                            .map(halquen_domain::PermissionSessionScope::Chat)
                    });
                let agent_id = proposal
                    .context
                    .agent
                    .as_ref()
                    .map(|identity| &identity.agent_id);
                if self
                    .database
                    .claim_permission_for_proposal(
                        &proposal,
                        permission_session.as_ref(),
                        agent_id,
                        started_at_ms,
                    )
                    .map_err(internal_error)?
                    .is_none()
                {
                    return Err(ProtocolErrorBody {
                        code: ProtocolErrorCode::PrivacyDenied,
                        message: "the exact permission is no longer active".to_owned(),
                    });
                }
            }
            audit_records.push(AuditRecord {
                id: AuditId::generate(),
                created_at_ms: started_at_ms,
                event: AuditEvent::AuthorizationCreated {
                    execution_id: execution_id.clone(),
                    capability_id: descriptor.id.clone(),
                    agent: proposal.context.agent.clone(),
                },
            });
            audit_records.push(AuditRecord {
                id: AuditId::generate(),
                created_at_ms: started_at_ms,
                event: AuditEvent::ExecutionStarted {
                    execution_id: execution_id.clone(),
                    capability_id: descriptor.id.clone(),
                },
            });
            let execution = if simulate {
                timeout(
                    Duration::from_millis(descriptor.timeout_ms),
                    self.dry_run_executor.execute(authorization),
                )
                .await
            } else {
                timeout(
                    Duration::from_millis(descriptor.timeout_ms),
                    self.executor.execute(authorization),
                )
                .await
            };
            match execution {
                Ok(Ok(outcome)) => {
                    let (status, result) = match outcome.code {
                        ExecutionResultCode::Simulated => {
                            (ExecutionStatus::DryRunSucceeded, SafeResultCode::Simulated)
                        }
                        ExecutionResultCode::Launched => {
                            (ExecutionStatus::Succeeded, SafeResultCode::Launched)
                        }
                    };
                    audit_records.push(AuditRecord {
                        id: AuditId::generate(),
                        created_at_ms: now_ms(),
                        event: AuditEvent::ExecutionCompleted {
                            execution_id: execution_id.clone(),
                            capability_id: descriptor.id.clone(),
                            result_code: Some(result),
                        },
                    });
                    (status, Some(result), None, None)
                }
                Ok(Err(error)) => {
                    let code = execution_error_code(&error).to_owned();
                    audit_records.push(AuditRecord {
                        id: AuditId::generate(),
                        created_at_ms: now_ms(),
                        event: AuditEvent::ExecutionFailed {
                            execution_id: execution_id.clone(),
                            capability_id: descriptor.id.clone(),
                            error_code: code.clone(),
                        },
                    });
                    (
                        ExecutionStatus::Failed,
                        None,
                        Some(code),
                        Some("executor rejected the authorized request".to_owned()),
                    )
                }
                Err(_) => {
                    let code = "capability_timeout".to_owned();
                    audit_records.push(AuditRecord {
                        id: AuditId::generate(),
                        created_at_ms: now_ms(),
                        event: AuditEvent::ExecutionTimedOut {
                            execution_id: execution_id.clone(),
                            capability_id: descriptor.id.clone(),
                            error_code: code.clone(),
                        },
                    });
                    (
                        ExecutionStatus::TimedOut,
                        None,
                        Some(code),
                        Some("capability execution exceeded its trusted deadline".to_owned()),
                    )
                }
            }
        } else {
            let event = match decision.outcome {
                PolicyOutcome::Confirm => AuditEvent::ConfirmationRequired {
                    execution_id: execution_id.clone(),
                    capability_id: descriptor.id.clone(),
                },
                PolicyOutcome::Deny | PolicyOutcome::Allow => AuditEvent::ActionDenied {
                    execution_id: execution_id.clone(),
                    capability_id: descriptor.id.clone(),
                },
            };
            audit_records.push(AuditRecord {
                id: AuditId::generate(),
                created_at_ms: now_ms(),
                event,
            });
            (ExecutionStatus::NotExecuted, None, None, None)
        };
        let finished_at_ms = now_ms().max(started_at_ms);
        let receipt = ExecutionReceipt {
            execution_id: execution_id.clone(),
            capability_id: descriptor.id.clone(),
            capability_version: descriptor.version,
            started_at_ms,
            finished_at_ms,
            policy_decision: decision.clone(),
            status,
            reversible: descriptor.reversible,
            result_code,
            error_code: error_code.clone(),
            sanitized_error,
            compensation_reference: None,
        };

        self.database
            .record_execution(&receipt, &audit_records)
            .map_err(internal_error)?;

        Ok(if simulate {
            ProtocolResponse::DryRun { decision, receipt }
        } else {
            ProtocolResponse::Execution { decision, receipt }
        })
    }

    pub(crate) fn policy_context(
        &self,
        proposal: &ActionProposal,
        session_id: Option<&halquen_domain::ChatSessionId>,
    ) -> Result<PolicyContext, ProtocolErrorBody> {
        let timestamp = now_ms();
        let mut context = PolicyContext::default();
        context.set_action_context(proposal.context.clone());
        context.set_profile(self.database.security_profile().map_err(internal_error)?);
        context.set_now_ms(timestamp);
        context.set_session_id(session_id.cloned());
        let permission_session = if let Some(identity) = &proposal.context.agent {
            Some(PermissionSessionScope::Agent(identity.session_id.clone()))
        } else {
            session_id.cloned().map(PermissionSessionScope::Chat)
        };
        let agent_id = proposal
            .context
            .agent
            .as_ref()
            .map(|identity| &identity.agent_id);
        if let Some(grant) = self
            .database
            .active_permission_grant_for_proposal(
                proposal,
                permission_session.as_ref(),
                agent_id,
                timestamp,
            )
            .map_err(internal_error)?
        {
            context.add_permission_grant(grant);
        }
        Ok(context)
    }

    pub(crate) fn descriptor_for(
        &self,
        action: &halquen_domain::ActionRequest,
    ) -> Result<&CapabilityDescriptor, ProtocolErrorBody> {
        let descriptor =
            self.registry
                .get(&action.capability_id)
                .ok_or_else(|| ProtocolErrorBody {
                    code: ProtocolErrorCode::NotFound,
                    message: "capability is not registered".to_owned(),
                })?;
        if action.arguments.kind() != descriptor.arguments {
            return Err(ProtocolErrorBody {
                code: ProtocolErrorCode::InvalidAction,
                message: "action arguments do not match the capability contract".to_owned(),
            });
        }
        Ok(descriptor)
    }
}

fn invalid_action_context() -> ProtocolErrorBody {
    ProtocolErrorBody {
        code: ProtocolErrorCode::InvalidAction,
        message: "action provenance or resource context is invalid".to_owned(),
    }
}

pub(crate) fn execution_error_code(error: &ExecutionError) -> &'static str {
    match error {
        ExecutionError::InvalidAuthorization => "executor_contract_rejected",
        ExecutionError::UnknownApplication => "application_not_registered",
        ExecutionError::ExecutableIdentityChanged => "executable_identity_changed",
        ExecutionError::SpawnFailed => "application_spawn_failed",
        ExecutionError::RegistryUnavailable => "application_registry_unavailable",
    }
}

pub(crate) fn internal_error(error: impl std::fmt::Display) -> ProtocolErrorBody {
    let _ = error;
    ProtocolErrorBody {
        code: ProtocolErrorCode::Internal,
        message: "internal core operation failed".to_owned(),
    }
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use halquen_capabilities::{ExecutionError, ExecutionOutcome};
    use halquen_domain::{
        ActionArgumentKind, ActionArguments, ActionRequest, CapabilityId, ConfirmationPolicy,
        EntityId, PermissionEffect, PermissionLifetime, ResourceClassification, ResourceDescriptor,
        ResourceKind, RiskClass,
    };
    use halquen_policy::{ExecutionAuthorization, PolicyOutcome};
    use halquen_protocol::PermissionGrantUpsert;

    use super::*;

    struct CountingExecutor {
        calls: Cell<u32>,
        delay_ms: u64,
    }

    impl Executor for CountingExecutor {
        async fn execute(
            &self,
            _authorization: ExecutionAuthorization,
        ) -> Result<ExecutionOutcome, ExecutionError> {
            self.calls.set(self.calls.get() + 1);
            if self.delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            }
            Ok(ExecutionOutcome {
                code: ExecutionResultCode::Simulated,
            })
        }
    }

    fn capability(id: &str, risk: RiskClass) -> CapabilityDescriptor {
        CapabilityDescriptor {
            id: CapabilityId::new(id).unwrap(),
            version: 1,
            description: "Pipeline test capability".to_owned(),
            risk,
            side_effect: risk != RiskClass::ReadOnly,
            idempotent: true,
            reversible: risk == RiskClass::ReversibleLocalWrite,
            scope_requirements: Vec::new(),
            confirmation: ConfirmationPolicy::RiskBased,
            timeout_ms: 1_000,
            arguments: ActionArgumentKind::None,
        }
    }

    fn service(descriptors: Vec<CapabilityDescriptor>) -> HalquenService<CountingExecutor> {
        let mut registry = CapabilityRegistry::new();
        for descriptor in descriptors {
            registry.register(descriptor).unwrap();
        }
        HalquenService::from_parts(
            registry,
            PolicyEngine::new(),
            CountingExecutor {
                calls: Cell::new(0),
                delay_ms: 0,
            },
            Database::open_in_memory().unwrap(),
        )
    }

    fn execute(id: &str) -> RequestEnvelope {
        RequestEnvelope {
            version: PROTOCOL_VERSION,
            request_id: "request:test".to_owned(),
            request: ProtocolRequest::ExecuteAction {
                action: ActionRequest::new(CapabilityId::new(id).unwrap(), ActionArguments::None),
            },
        }
    }

    #[tokio::test]
    async fn allowed_request_runs_executor_and_persists_audit() {
        let mut service = service(vec![capability("test.read", RiskClass::ReadOnly)]);
        let response = service.handle(execute("test.read")).await;
        let execution_id = match response.response {
            ProtocolResponse::Execution { receipt, .. } => {
                assert_eq!(receipt.status, ExecutionStatus::DryRunSucceeded);
                receipt.execution_id
            }
            other => panic!("unexpected response: {other:?}"),
        };
        assert_eq!(service.executor.calls.get(), 1);
        let stats = service.database().audit_stats().unwrap();
        assert_eq!(stats.executions, 1);
        assert_eq!(stats.records, 6);
        assert_eq!(
            service
                .database()
                .audit_event_kinds(execution_id.as_str())
                .unwrap(),
            vec![
                "proposal_created".to_owned(),
                "action_requested".to_owned(),
                "policy_evaluated".to_owned(),
                "authorization_created".to_owned(),
                "execution_started".to_owned(),
                "execution_completed".to_owned(),
            ]
        );
    }

    #[tokio::test]
    async fn confirm_and_deny_never_reach_executor_or_started_audit() {
        for (id, risk, expected) in [
            (
                "test.external",
                RiskClass::ExternalSideEffect,
                PolicyOutcome::Confirm,
            ),
            (
                "test.privileged",
                RiskClass::Privileged,
                PolicyOutcome::Deny,
            ),
        ] {
            let mut service = service(vec![capability(id, risk)]);
            let response = service.handle(execute(id)).await;
            let execution_id = match response.response {
                ProtocolResponse::Execution { decision, receipt } => {
                    assert_eq!(decision.outcome, expected);
                    assert_eq!(receipt.status, ExecutionStatus::NotExecuted);
                    receipt.execution_id
                }
                other => panic!("unexpected response: {other:?}"),
            };
            assert_eq!(service.executor.calls.get(), 0);
            let kinds = service
                .database()
                .audit_event_kinds(execution_id.as_str())
                .unwrap();
            assert!(!kinds.iter().any(|kind| kind == "execution_started"));
            assert!(kinds.iter().any(|kind| {
                kind == if expected == PolicyOutcome::Confirm {
                    "confirmation_required"
                } else {
                    "action_denied"
                }
            }));
        }
    }

    #[tokio::test]
    async fn direct_open_app_exact_deny_blocks_the_executor() {
        let mut service = service(vec![open_app_descriptor()]);
        let app = EntityId::new("app:safe-fixture").unwrap();
        service
            .upsert_permission_grant(PermissionGrantUpsert {
                id: None,
                effect: PermissionEffect::Deny,
                lifetime: PermissionLifetime::Always,
                capability_id: CapabilityId::new("system.open_app").unwrap(),
                arguments: ActionArguments::OpenApp { app: app.clone() },
                resources: vec![ResourceDescriptor {
                    kind: ResourceKind::Application,
                    identifier: app.as_str().to_owned(),
                    classification: ResourceClassification::Local,
                }],
                destination: None,
                session: None,
                agent_id: None,
                expires_at_ms: None,
            })
            .unwrap();

        let response = service
            .handle(RequestEnvelope {
                version: PROTOCOL_VERSION,
                request_id: "request:open-app-deny".to_owned(),
                request: ProtocolRequest::ExecuteAction {
                    action: ActionRequest::new(
                        CapabilityId::new("system.open_app").unwrap(),
                        ActionArguments::OpenApp { app },
                    ),
                },
            })
            .await;

        match response.response {
            ProtocolResponse::Execution { decision, receipt } => {
                assert_eq!(decision.outcome, PolicyOutcome::Deny);
                assert_eq!(decision.reason, PolicyReason::PersistentExactDeny);
                assert_eq!(receipt.status, ExecutionStatus::NotExecuted);
            }
            other => panic!("unexpected response: {other:?}"),
        }
        assert_eq!(service.executor.calls.get(), 0);
    }

    #[tokio::test]
    async fn slow_executor_is_cancelled_by_descriptor_deadline() {
        let mut descriptor = capability("test.slow", RiskClass::ReadOnly);
        descriptor.timeout_ms = 5;
        let mut registry = CapabilityRegistry::new();
        registry.register(descriptor).unwrap();
        let mut service = HalquenService::from_parts(
            registry,
            PolicyEngine::new(),
            CountingExecutor {
                calls: Cell::new(0),
                delay_ms: 100,
            },
            Database::open_in_memory().unwrap(),
        );

        let response = service.handle(execute("test.slow")).await;
        let execution_id = match response.response {
            ProtocolResponse::Execution { receipt, .. } => {
                assert_eq!(receipt.status, ExecutionStatus::TimedOut);
                receipt.execution_id
            }
            other => panic!("unexpected response: {other:?}"),
        };
        assert_eq!(service.executor.calls.get(), 1);
        assert_eq!(
            service
                .database()
                .audit_event_kinds(execution_id.as_str())
                .unwrap(),
            vec![
                "proposal_created".to_owned(),
                "action_requested".to_owned(),
                "policy_evaluated".to_owned(),
                "authorization_created".to_owned(),
                "execution_started".to_owned(),
                "execution_timed_out".to_owned(),
            ]
        );
    }
}
