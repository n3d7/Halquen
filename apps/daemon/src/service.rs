use std::time::{Duration, SystemTime, UNIX_EPOCH};

use halquen_audit::{
    AuditEvent, AuditRecord, ExecutionReceipt, ExecutionStatus, SafeResultCode,
};
use halquen_capabilities::{
    CapabilityRegistry, ExecutionResultCode, Executor, open_app_descriptor,
};
use halquen_domain::{AuditId, CapabilityDescriptor, ExecutionId};
use halquen_policy::{PolicyEngine, PolicyOutcome};
use halquen_protocol::{
    HealthStatus, PROTOCOL_VERSION, ProtocolErrorBody, ProtocolErrorCode, ProtocolRequest,
    ProtocolResponse, RequestEnvelope, ResponseEnvelope,
};
use halquen_storage::Database;
use tokio::time::timeout;

pub struct HalquenService<E> {
    registry: CapabilityRegistry,
    policy: PolicyEngine,
    executor: E,
    database: Database,
}

impl<E: Executor> HalquenService<E> {
    pub fn new(executor: E, database: Database) -> Result<Self, Box<dyn std::error::Error>> {
        let mut registry = CapabilityRegistry::new();
        registry.register(open_app_descriptor())?;
        Ok(Self {
            registry,
            policy: PolicyEngine::new(),
            executor,
            database,
        })
    }

    pub fn from_parts(
        registry: CapabilityRegistry,
        policy: PolicyEngine,
        executor: E,
        database: Database,
    ) -> Self {
        Self {
            registry,
            policy,
            executor,
            database,
        }
    }

    pub fn database(&self) -> &Database {
        &self.database
    }

    pub async fn handle(&mut self, envelope: RequestEnvelope) -> ResponseEnvelope {
        let request_id = envelope.request_id;
        let response = match envelope.request {
            ProtocolRequest::Health => self.health(),
            ProtocolRequest::ListCapabilities => Ok(ProtocolResponse::Capabilities {
                capabilities: self.registry.list().cloned().collect(),
            }),
            ProtocolRequest::GetCapability { capability_id } => {
                Ok(ProtocolResponse::Capability {
                    capability: self.registry.get(&capability_id).cloned(),
                })
            }
            ProtocolRequest::EvaluateAction { action } => self.evaluate_action(action),
            ProtocolRequest::DryRunAction { action } => self.dry_run_action(action).await,
            ProtocolRequest::MemoryStats => self.memory_stats(),
            ProtocolRequest::AuditStats => self.audit_stats(),
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
        action: halquen_domain::ActionRequest,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let descriptor = self.descriptor_for(&action)?.clone();
        let decision = self.policy.evaluate(&descriptor);
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
        Ok(ProtocolResponse::Evaluation {
            decision,
        })
    }

    async fn dry_run_action(
        &mut self,
        action: halquen_domain::ActionRequest,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let descriptor = self.descriptor_for(&action)?.clone();
        let execution_id = ExecutionId::generate();
        let started_at_ms = now_ms();
        let evaluation = self
            .policy
            .authorize(&descriptor, action, execution_id.clone());
        let decision = evaluation.decision.clone();
        let mut audit_records = vec![
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

        let (status, result_code, error_code, sanitized_error) =
            if let Some(authorization) = evaluation.into_authorization() {
                audit_records.push(AuditRecord {
                    id: AuditId::generate(),
                    created_at_ms: started_at_ms,
                    event: AuditEvent::ExecutionStarted {
                        execution_id: execution_id.clone(),
                        capability_id: descriptor.id.clone(),
                    },
                });
                match timeout(
                    Duration::from_millis(descriptor.timeout_ms),
                    self.executor.execute(authorization),
                )
                .await
                {
                    Ok(Ok(outcome)) => {
                        let result = match outcome.code {
                            ExecutionResultCode::Simulated => SafeResultCode::Simulated,
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
                        (ExecutionStatus::DryRunSucceeded, Some(result), None, None)
                    }
                    Ok(Err(_)) => {
                        let code = "executor_contract_rejected".to_owned();
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

        Ok(ProtocolResponse::DryRun {
            decision,
            receipt,
        })
    }

    fn descriptor_for(
        &self,
        action: &halquen_domain::ActionRequest,
    ) -> Result<&CapabilityDescriptor, ProtocolErrorBody> {
        let descriptor = self.registry.get(&action.capability_id).ok_or_else(|| {
            ProtocolErrorBody {
                code: ProtocolErrorCode::NotFound,
                message: "capability is not registered".to_owned(),
            }
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

fn internal_error(error: impl std::fmt::Display) -> ProtocolErrorBody {
    let _ = error;
    ProtocolErrorBody {
        code: ProtocolErrorCode::Internal,
        message: "internal core operation failed".to_owned(),
    }
}

fn now_ms() -> i64 {
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
        RiskClass,
    };
    use halquen_policy::{ExecutionAuthorization, PolicyOutcome};

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

    fn dry_run(id: &str) -> RequestEnvelope {
        RequestEnvelope {
            version: PROTOCOL_VERSION,
            request_id: "request:test".to_owned(),
            request: ProtocolRequest::DryRunAction {
                action: ActionRequest::new(
                    CapabilityId::new(id).unwrap(),
                    ActionArguments::None,
                ),
            },
        }
    }

    #[tokio::test]
    async fn allowed_request_runs_executor_and_persists_audit() {
        let mut service = service(vec![capability("test.read", RiskClass::ReadOnly)]);
        let response = service.handle(dry_run("test.read")).await;
        let execution_id = match response.response {
            ProtocolResponse::DryRun { receipt, .. } => {
                assert_eq!(receipt.status, ExecutionStatus::DryRunSucceeded);
                receipt.execution_id
            }
            other => panic!("unexpected response: {other:?}"),
        };
        assert_eq!(service.executor.calls.get(), 1);
        let stats = service.database().audit_stats().unwrap();
        assert_eq!(stats.executions, 1);
        assert_eq!(stats.records, 4);
        assert_eq!(
            service
                .database()
                .audit_event_kinds(execution_id.as_str())
                .unwrap(),
            vec![
                "action_requested".to_owned(),
                "policy_evaluated".to_owned(),
                "execution_started".to_owned(),
                "execution_completed".to_owned(),
            ]
        );
    }

    #[tokio::test]
    async fn confirm_and_deny_never_reach_executor_or_started_audit() {
        for (id, risk, expected) in [
            ("test.external", RiskClass::ExternalSideEffect, PolicyOutcome::Confirm),
            ("test.privileged", RiskClass::Privileged, PolicyOutcome::Deny),
        ] {
            let mut service = service(vec![capability(id, risk)]);
            let response = service.handle(dry_run(id)).await;
            let execution_id = match response.response {
                ProtocolResponse::DryRun { decision, receipt } => {
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
                kind
                    == if expected == PolicyOutcome::Confirm {
                        "confirmation_required"
                    } else {
                        "action_denied"
                    }
            }));
        }
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

        let response = service.handle(dry_run("test.slow")).await;
        let execution_id = match response.response {
            ProtocolResponse::DryRun { receipt, .. } => {
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
                "action_requested".to_owned(),
                "policy_evaluated".to_owned(),
                "execution_started".to_owned(),
                "execution_timed_out".to_owned(),
            ]
        );
    }
}
