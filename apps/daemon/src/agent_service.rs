use halquen_ai::{
    AgentBrokerDisposition, AgentBrokerProposalResult, AgentCapabilityView, AgentHostError,
};
use halquen_audit::{AuditEvent, AuditRecord, ExecutionStatus};
use halquen_capabilities::Executor;
use halquen_domain::{
    ActionArguments, AgentExecutionIdentity, AgentInstanceId, AgentSession, AgentSessionId,
    AgentSessionStatus, AuditId, ResourceClassification, ResourceDescriptor, ResourceKind,
};
use halquen_policy::PolicyOutcome;
use halquen_protocol::{
    AgentProposalDisposition, AgentProposalResult, AgentRunRequest, AgentRunResult,
    ProtocolErrorBody, ProtocolErrorCode, ProtocolResponse,
};

use crate::service::{HalquenService, internal_error, now_ms};

impl<E: Executor> HalquenService<E> {
    pub(crate) async fn run_agent(
        &mut self,
        request: AgentRunRequest,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        if request.input.trim().is_empty() || request.input.len() > 64 * 1024 {
            return Err(validation("agent input must be between 1 and 65536 bytes"));
        }
        let configuration = self
            .database
            .list_agents(100)
            .map_err(internal_error)?
            .into_iter()
            .find(|agent| agent.id == request.agent_id)
            .ok_or_else(|| ProtocolErrorBody {
                code: ProtocolErrorCode::NotFound,
                message: "agent is not configured".to_owned(),
            })?;
        let started_at_ms = now_ms();
        let identity = AgentExecutionIdentity {
            agent_id: configuration.id.clone(),
            instance_id: AgentInstanceId::generate(),
            session_id: AgentSessionId::generate(),
        };
        let mut session = AgentSession {
            id: identity.session_id.clone(),
            agent_id: identity.agent_id.clone(),
            instance_id: identity.instance_id.clone(),
            daemon_session_id: self.daemon_session_id.clone(),
            status: AgentSessionStatus::Running,
            started_at_ms,
            ended_at_ms: None,
        };
        self.database
            .begin_agent_session(&session)
            .map_err(internal_error)?;
        if let Err(error) = self.database.append_audit(&AuditRecord {
            id: AuditId::generate(),
            created_at_ms: started_at_ms,
            event: AuditEvent::AgentSessionStarted {
                session_id: session.id.clone(),
                agent: identity.clone(),
            },
        }) {
            let _ = self.finish_agent_session(&mut session, AgentSessionStatus::Failed);
            return Err(internal_error(error));
        }

        let capabilities = self
            .registry
            .list()
            .map(|descriptor| AgentCapabilityView {
                id: descriptor.id.clone(),
                version: descriptor.version,
                description: descriptor.description.clone(),
                risk: descriptor.risk,
                arguments: descriptor.arguments,
            })
            .collect::<Vec<_>>();
        let host = self.agent_host;
        let mut running = match host
            .start(&configuration, identity, &request.input, &capabilities)
            .await
        {
            Ok(running) => running,
            Err(error) => {
                let status = agent_error_status(&error);
                self.finish_agent_session(&mut session, status)?;
                return Err(agent_error(error));
            }
        };
        let invocation = match running.receive_proposals().await {
            Ok(invocation) => invocation,
            Err(error) => {
                let status = agent_error_status(&error);
                self.finish_agent_session(&mut session, status)?;
                return Err(agent_error(error));
            }
        };

        let mut public_results = Vec::with_capacity(invocation.proposals.len());
        let mut broker_results = Vec::with_capacity(invocation.proposals.len());
        for (index, mut proposal) in invocation.proposals.into_iter().enumerate() {
            let index = match u16::try_from(index) {
                Ok(index) => index,
                Err(_) => {
                    running.terminate().await;
                    self.finish_agent_session(&mut session, AgentSessionStatus::Failed)?;
                    return Err(validation("agent returned too many proposals"));
                }
            };
            let capability_id = proposal.action.capability_id.clone();
            if let ActionArguments::OpenApp { app } = &proposal.action.arguments {
                let identifier = app.to_string();
                let classification = match self
                    .database
                    .resource_label_for(ResourceKind::Application, &identifier)
                {
                    Ok(label) => {
                        label.map_or(ResourceClassification::Local, |label| label.classification)
                    }
                    Err(error) => {
                        running.terminate().await;
                        self.finish_agent_session(&mut session, AgentSessionStatus::Failed)?;
                        return Err(internal_error(error));
                    }
                };
                proposal.context.resources.push(ResourceDescriptor {
                    kind: ResourceKind::Application,
                    identifier,
                    classification,
                });
                if proposal.context.validate().is_err() {
                    running.terminate().await;
                    self.finish_agent_session(&mut session, AgentSessionStatus::Failed)?;
                    return Err(validation("agent proposal context is invalid"));
                }
            }
            let response = self.execute_proposal(proposal, None).await;
            let (public, broker) = match response {
                Ok(ProtocolResponse::Execution { decision, receipt })
                    if decision.outcome == PolicyOutcome::Allow =>
                {
                    let disposition = if receipt.status == ExecutionStatus::DryRunSucceeded {
                        AgentProposalDisposition::Simulated
                    } else if receipt.status == ExecutionStatus::Succeeded {
                        AgentProposalDisposition::Executed
                    } else {
                        AgentProposalDisposition::Failed
                    };
                    let broker_disposition = match disposition {
                        AgentProposalDisposition::Executed => AgentBrokerDisposition::Executed,
                        AgentProposalDisposition::Simulated => AgentBrokerDisposition::Simulated,
                        AgentProposalDisposition::Failed => AgentBrokerDisposition::Failed,
                        AgentProposalDisposition::ConfirmationRequired => {
                            AgentBrokerDisposition::ConfirmationRequired
                        }
                        AgentProposalDisposition::Denied => AgentBrokerDisposition::Denied,
                    };
                    let execution_id = Some(receipt.execution_id.clone());
                    (
                        AgentProposalResult {
                            index,
                            capability_id: capability_id.clone(),
                            disposition,
                            execution_id: execution_id.clone(),
                            confirmation: None,
                            message: "daemon broker completed the authorized proposal".to_owned(),
                        },
                        AgentBrokerProposalResult {
                            index,
                            capability_id,
                            disposition: broker_disposition,
                            execution_id: execution_id.map(|id| id.to_string()),
                            confirmation_id: None,
                            message: "daemon broker completed the authorized proposal".to_owned(),
                        },
                    )
                }
                Ok(ProtocolResponse::Execution { decision, receipt })
                    if decision.outcome == PolicyOutcome::Confirm =>
                {
                    let message = "proposal requires an exact user grant; this one-shot agent session cannot self-confirm".to_owned();
                    (
                        AgentProposalResult {
                            index,
                            capability_id: capability_id.clone(),
                            disposition: AgentProposalDisposition::ConfirmationRequired,
                            execution_id: Some(receipt.execution_id.clone()),
                            confirmation: None,
                            message: message.clone(),
                        },
                        AgentBrokerProposalResult {
                            index,
                            capability_id,
                            disposition: AgentBrokerDisposition::ConfirmationRequired,
                            execution_id: Some(receipt.execution_id.to_string()),
                            confirmation_id: None,
                            message,
                        },
                    )
                }
                Ok(ProtocolResponse::Execution { receipt, .. }) => {
                    let message = "daemon policy denied the proposal".to_owned();
                    (
                        AgentProposalResult {
                            index,
                            capability_id: capability_id.clone(),
                            disposition: AgentProposalDisposition::Denied,
                            execution_id: Some(receipt.execution_id.clone()),
                            confirmation: None,
                            message: message.clone(),
                        },
                        AgentBrokerProposalResult {
                            index,
                            capability_id,
                            disposition: AgentBrokerDisposition::Denied,
                            execution_id: Some(receipt.execution_id.to_string()),
                            confirmation_id: None,
                            message,
                        },
                    )
                }
                Ok(_) | Err(_) => {
                    let message = "daemon broker rejected an invalid proposal".to_owned();
                    (
                        AgentProposalResult {
                            index,
                            capability_id: capability_id.clone(),
                            disposition: AgentProposalDisposition::Failed,
                            execution_id: None,
                            confirmation: None,
                            message: message.clone(),
                        },
                        AgentBrokerProposalResult {
                            index,
                            capability_id,
                            disposition: AgentBrokerDisposition::Failed,
                            execution_id: None,
                            confirmation_id: None,
                            message,
                        },
                    )
                }
            };
            public_results.push(public);
            broker_results.push(broker);
        }

        let completion = match running.complete(&broker_results).await {
            Ok(completion) => completion,
            Err(error) => {
                let status = agent_error_status(&error);
                self.finish_agent_session(&mut session, status)?;
                return Err(agent_error(error));
            }
        };
        self.finish_agent_session(&mut session, AgentSessionStatus::Completed)?;
        Ok(ProtocolResponse::AgentRun {
            result: AgentRunResult {
                session,
                message: invocation.message,
                proposals: public_results,
                stderr_bytes: u32::try_from(completion.stderr_bytes).unwrap_or(u32::MAX),
            },
        })
    }

    fn finish_agent_session(
        &mut self,
        session: &mut AgentSession,
        status: AgentSessionStatus,
    ) -> Result<(), ProtocolErrorBody> {
        let ended_at_ms = now_ms().max(session.started_at_ms);
        self.database
            .finish_agent_session(&session.id, status, ended_at_ms)
            .map_err(internal_error)?;
        session.status = status;
        session.ended_at_ms = Some(ended_at_ms);
        self.database
            .append_audit(&AuditRecord {
                id: AuditId::generate(),
                created_at_ms: ended_at_ms,
                event: AuditEvent::AgentSessionFinished {
                    session_id: session.id.clone(),
                    status,
                },
            })
            .map_err(internal_error)
    }
}

fn agent_error_status(error: &AgentHostError) -> AgentSessionStatus {
    match error {
        AgentHostError::TimedOut => AgentSessionStatus::TimedOut,
        AgentHostError::ProcessFailed | AgentHostError::Io => AgentSessionStatus::Crashed,
        _ => AgentSessionStatus::Failed,
    }
}

fn agent_error(error: AgentHostError) -> ProtocolErrorBody {
    let code = match error {
        AgentHostError::SandboxUnavailable | AgentHostError::ResourceLimitUnavailable => {
            ProtocolErrorCode::SandboxUnavailable
        }
        AgentHostError::InvalidConfiguration
        | AgentHostError::Disabled
        | AgentHostError::UnsupportedTransport
        | AgentHostError::UnsafeOptInRequired
        | AgentHostError::InvalidExecutable => ProtocolErrorCode::Validation,
        AgentHostError::SpawnFailed
        | AgentHostError::TimedOut
        | AgentHostError::OutputTooLarge
        | AgentHostError::MalformedOutput
        | AgentHostError::ProcessFailed
        | AgentHostError::Io => ProtocolErrorCode::Internal,
    };
    ProtocolErrorBody {
        code,
        message: "agent broker operation failed safely".to_owned(),
    }
}

fn validation(message: &str) -> ProtocolErrorBody {
    ProtocolErrorBody {
        code: ProtocolErrorCode::Validation,
        message: message.to_owned(),
    }
}
