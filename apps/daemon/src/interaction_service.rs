use std::sync::Arc;
use std::time::{Duration, Instant};

use halquen_ai::{
    AiError, AiRequest, ContextBuilder, ContextItem, ModelRouter, PromptComposer, PromptProfile,
    ProviderClient, RouteRequest, SecretError, SecretStore, validate_provider,
};
use halquen_audit::{AuditEvent, AuditRecord, ExecutionReceipt, ExecutionStatus, SafeResultCode};
use halquen_capabilities::{ExecutionResultCode, Executor};
use halquen_domain::{
    ActionArguments, ActionContext, ActionOrigin, ActionProposal, ActionRequest, ActivityEvent,
    ActivityId, ActivityKind, AiTaskType, AuditId, BehaviourEventId, BehaviourOutcome,
    CacheEntryId, CachedResponse, CapabilityId, ChatMessage, ChatMessageId, ChatOrigin, ChatRole,
    ChatRoute, ContextCategory, Correction, CorrectionId, DiagnosticEntry, DiagnosticSeverity,
    EvidenceId, ExecutionId, IntentUsageEvent, MemoryId, MemoryRevisionId, ModelSelection,
    PermissionEffect, PermissionGrant, PermissionId, PermissionLifetime, PermissionScope,
    PermissionSessionScope, PrivacyClass, ProposalId, Provider, ProviderId, ProviderStatus,
    ResourceClassification, ResourceDescriptor, ResourceKind, ResponseFeedback, TrustClass,
    UsageStats,
};
use halquen_memory::{
    BehaviourScorer, DEFAULT_RETENTION_MS, Evidence, IntentResolution, MemoryItem, MemoryKind,
    MemoryRevision, MemoryValue,
};
use halquen_policy::PolicyOutcome;
use halquen_protocol::{
    ChatRequest, ChatResult, ConfirmationPersistence, ConfirmationPrompt, ConfirmationResult,
    DiagnosticsSnapshot, MemoryMutationReceipt, MemoryQuery, MemoryStateUpdate, ModelUpsert,
    PROTOCOL_VERSION, PromptPreview, ProtocolErrorBody, ProtocolErrorCode, ProtocolResponse,
    ProviderTestStatus, ProviderUpsert,
};
use tokio::sync::watch;
use tokio::time::timeout;
use zeroize::Zeroizing;

use crate::chat::{LocalIntent, application_entity, normalize_request, resolve_local};
use crate::service::{HalquenService, PendingConfirmation, internal_error, now_ms};

const CONFIRMATION_TTL_MS: i64 = 5 * 60 * 1_000;
const RESPONSE_CANDIDATE_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1_000;

impl<E: Executor> HalquenService<E> {
    pub fn set_integrations(
        &mut self,
        provider_client: Arc<dyn ProviderClient>,
        secret_store: Arc<dyn SecretStore>,
    ) {
        self.provider_client = provider_client;
        self.secret_store = secret_store;
    }

    pub(crate) async fn chat(
        &mut self,
        request: ChatRequest,
        correlation_id: &str,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let (_sender, receiver) = watch::channel(false);
        self.chat_with_cancellation(request, correlation_id, receiver)
            .await
    }

    pub(crate) async fn chat_with_cancellation(
        &mut self,
        request: ChatRequest,
        correlation_id: &str,
        mut cancellation: watch::Receiver<bool>,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let message = request.message.trim();
        if message.is_empty() || message.len() > 16_384 {
            return Err(validation("message must contain between 1 and 16384 bytes"));
        }
        let started = Instant::now();
        let timestamp = now_ms();
        let session = self
            .database
            .ensure_chat_session(request.session_id, message, timestamp)
            .map_err(internal_error)?;
        let user_message = ChatMessage {
            id: ChatMessageId::generate(),
            session_id: session.id.clone(),
            role: ChatRole::User,
            content: message.to_owned(),
            origin: ChatOrigin::User,
            route: None,
            provider_id: None,
            model_id: None,
            input_tokens: None,
            output_tokens: None,
            latency_ms: None,
            reusable_candidate_id: None,
            created_at_ms: timestamp,
        };
        self.database
            .append_chat_message(&user_message)
            .map_err(internal_error)?;
        self.activity(
            Some(session.id.clone()),
            correlation_id,
            ActivityKind::RequestReceived,
            "Request received",
            None,
        )?;

        if let Some(intent) = resolve_local(message) {
            return self
                .handle_local_intent(intent, session, user_message, correlation_id, started)
                .await;
        }

        let normalized = normalize_request(message);
        let settings = self
            .database
            .application_settings()
            .map_err(internal_error)?;
        if settings.prefer_cached_local
            && let Some(entry) = self
                .database
                .cached_response(&normalized, "global", timestamp)
                .map_err(internal_error)?
        {
            let estimated = u64::try_from(
                message
                    .chars()
                    .count()
                    .saturating_add(entry.response.chars().count())
                    / 4,
            )
            .unwrap_or(u64::MAX);
            self.database
                .record_cache_hit(&entry.id, timestamp, estimated)
                .map_err(internal_error)?;
            self.activity(
                Some(session.id.clone()),
                correlation_id,
                ActivityKind::CacheHit,
                "Reusable response resolved locally",
                Some("No AI provider was contacted".to_owned()),
            )?;
            let assistant = assistant_message(
                &session.id,
                entry.response,
                ChatOrigin::Cache,
                ChatRoute::ResponseCache,
                started,
                None,
                None,
                None,
                Some(entry.id),
            );
            return self.finish_chat(session, user_message, assistant, None);
        }

        self.activity(
            Some(session.id.clone()),
            correlation_id,
            ActivityKind::CacheMiss,
            "No reusable local response matched",
            None,
        )?;
        self.ai_fallback(
            request.model_selection,
            session,
            user_message,
            correlation_id,
            started,
            &mut cancellation,
        )
        .await
    }

    async fn handle_local_intent(
        &mut self,
        intent: LocalIntent,
        session: halquen_domain::ChatSession,
        user_message: ChatMessage,
        correlation_id: &str,
        started: Instant,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let intent = match intent {
            LocalIntent::ContextualOpenApp => {
                let timestamp = now_ms();
                let events = self
                    .database
                    .recent_intent_usage(
                        "open_application",
                        "application",
                        timestamp.saturating_sub(DEFAULT_RETENTION_MS),
                        512,
                    )
                    .map_err(internal_error)?;
                match BehaviourScorer::default().resolve(&events, timestamp) {
                    IntentResolution::Resolved(candidate) => LocalIntent::OpenApp {
                        display_name: display_for_entity(&candidate.entity_id),
                        entity_id: candidate.entity_id,
                    },
                    IntentResolution::Ambiguous(candidates) => {
                        let detail = candidates
                            .iter()
                            .map(|candidate| {
                                format!(
                                    "{} {:.2}",
                                    display_for_entity(&candidate.entity_id),
                                    f32::from(candidate.score_permille) / 1_000.0
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(" · ");
                        self.database
                            .add_usage(UsageStats {
                                clarifications: 1,
                                ..UsageStats::default()
                            })
                            .map_err(internal_error)?;
                        self.activity(
                            Some(session.id.clone()),
                            correlation_id,
                            ActivityKind::LocalRouteHit,
                            "Clarification requested for contextual application intent",
                            Some(detail.clone()),
                        )?;
                        let assistant = assistant_message(
                            &session.id,
                            if detail.is_empty() {
                                "Which application should I open?".to_owned()
                            } else {
                                format!(
                                    "Which application did you mean? Recent candidates: {detail}"
                                )
                            },
                            ChatOrigin::Local,
                            ChatRoute::Clarification,
                            started,
                            None,
                            None,
                            None,
                            None,
                        );
                        return self.finish_chat(session, user_message, assistant, None);
                    }
                    IntentResolution::Unknown => {
                        self.database
                            .add_usage(UsageStats {
                                clarifications: 1,
                                ..UsageStats::default()
                            })
                            .map_err(internal_error)?;
                        let assistant = assistant_message(
                            &session.id,
                            "Which application should I open? I do not have enough recent evidence to choose safely."
                                .to_owned(),
                            ChatOrigin::Local,
                            ChatRoute::Clarification,
                            started,
                            None,
                            None,
                            None,
                            None,
                        );
                        return self.finish_chat(session, user_message, assistant, None);
                    }
                }
            }
            LocalIntent::CorrectOpenApp { rejected, accepted } => {
                let timestamp = now_ms();
                let rejected_entity = application_entity(&rejected)
                    .ok_or_else(|| validation("corrected application name is invalid"))?;
                let accepted_entity = application_entity(&accepted)
                    .ok_or_else(|| validation("corrected application name is invalid"))?;
                for (entity_id, outcome) in [
                    (rejected_entity, BehaviourOutcome::CorrectionRejected),
                    (
                        accepted_entity.clone(),
                        BehaviourOutcome::CorrectionAccepted,
                    ),
                ] {
                    self.database
                        .record_intent_usage(&IntentUsageEvent {
                            id: BehaviourEventId::generate(),
                            intent: "open_application".to_owned(),
                            entity_id,
                            outcome,
                            context_class: "application".to_owned(),
                            created_at_ms: timestamp,
                        })
                        .map_err(internal_error)?;
                }
                self.database
                    .record_correction(&Correction {
                        id: CorrectionId::generate(),
                        target_id: "intent:open_application".to_owned(),
                        correction_summary: format!("{rejected} -> {accepted}"),
                        created_at_ms: timestamp,
                    })
                    .map_err(internal_error)?;
                LocalIntent::OpenApp {
                    display_name: accepted,
                    entity_id: accepted_entity,
                }
            }
            other => other,
        };

        match intent {
            LocalIntent::OpenApp {
                mut display_name,
                mut entity_id,
            } => {
                if let Some((_item, revision)) = self
                    .database
                    .preference_by_key(&display_name)
                    .map_err(internal_error)?
                    && let MemoryValue::Preference { value, .. } = revision.value
                    && let Some(alias_entity) = application_entity(&value)
                {
                    display_name = value;
                    entity_id = alias_entity;
                }
                let resource_id = entity_id.as_str().to_owned();
                let resource_classification = self
                    .database
                    .resource_label_for(ResourceKind::Application, &resource_id)
                    .map_err(internal_error)?
                    .map_or(ResourceClassification::Local, |label| label.classification);
                let action = ActionRequest::new(
                    CapabilityId::new("system.open_app")
                        .map_err(|_| internal_error("built-in capability identifier is invalid"))?,
                    ActionArguments::OpenApp { app: entity_id },
                );
                let proposal = ActionProposal::new(
                    action,
                    ActionContext::local_resolution(Some(correlation_id.to_owned())).with_resource(
                        ResourceDescriptor {
                            kind: ResourceKind::Application,
                            identifier: resource_id,
                            classification: resource_classification,
                        },
                    ),
                )
                .map_err(|_| validation("local action context is invalid"))?;
                let response = self
                    .execute_proposal(proposal.clone(), Some(&session.id))
                    .await?;
                let (content, confirmation) = match response {
                    ProtocolResponse::Execution { decision, receipt }
                        if decision.outcome == PolicyOutcome::Allow =>
                    {
                        self.database
                            .record_intent_usage(&IntentUsageEvent {
                                id: BehaviourEventId::generate(),
                                intent: "open_application".to_owned(),
                                entity_id: match &proposal.action.arguments {
                                    ActionArguments::OpenApp { app } => app.clone(),
                                    ActionArguments::None => {
                                        return Err(internal_error(
                                            "open-app proposal lost arguments",
                                        ));
                                    }
                                },
                                outcome: BehaviourOutcome::Success,
                                context_class: "application".to_owned(),
                                created_at_ms: now_ms(),
                            })
                            .map_err(internal_error)?;
                        self.activity(
                            Some(session.id.clone()),
                            correlation_id,
                            ActivityKind::ExecutionCompleted,
                            "Local capability execution completed",
                            Some(format!(
                                "{} v{} · {:?}",
                                receipt.capability_id, receipt.capability_version, receipt.status
                            )),
                        )?;
                        let content = if receipt.status == ExecutionStatus::DryRunSucceeded {
                            format!(
                                "Dry-run completed for {display_name}. No application was launched."
                            )
                        } else {
                            format!("Launch request completed for {display_name}.")
                        };
                        (content, None)
                    }
                    ProtocolResponse::Execution { decision, receipt }
                        if decision.outcome == PolicyOutcome::Confirm =>
                    {
                        let confirmation_id =
                            format!("confirmation:{}", halquen_domain::ProposalId::generate());
                        let expires_at_ms = now_ms().saturating_add(CONFIRMATION_TTL_MS);
                        self.pending_confirmations.insert(
                            confirmation_id.clone(),
                            PendingConfirmation {
                                request_execution_id: receipt.execution_id,
                                proposal: proposal.clone(),
                                title: format!("Open {display_name}"),
                                expires_at_ms,
                                session_id: Some(session.id.clone()),
                            },
                        );
                        self.activity(
                            Some(session.id.clone()),
                            correlation_id,
                            ActivityKind::ConfirmationRequired,
                            "Action requires confirmation",
                            Some("Allow once is available; dismissing does not confirm".to_owned()),
                        )?;
                        (
                            "Confirmation is required before this action can run.".to_owned(),
                            Some(ConfirmationPrompt {
                                confirmation_id,
                                title: format!("Open {display_name}"),
                                reason: "Policy requires explicit confirmation".to_owned(),
                                expires_at_ms,
                                operation: "system.open_app".to_owned(),
                                target: display_name.clone(),
                                destination: None,
                                origin: proposal.context.origin,
                                resource_classifications: proposal
                                    .context
                                    .resources
                                    .iter()
                                    .map(|resource| resource.classification)
                                    .collect(),
                                agent_id: None,
                                agent_session_id: None,
                            }),
                        )
                    }
                    ProtocolResponse::Execution { .. } => {
                        ("Halquen denied this action.".to_owned(), None)
                    }
                    _ => return Err(internal_error("unexpected action response")),
                };
                self.database
                    .add_usage(UsageStats {
                        local_resolutions: 1,
                        ..UsageStats::default()
                    })
                    .map_err(internal_error)?;
                self.activity(
                    Some(session.id.clone()),
                    correlation_id,
                    ActivityKind::LocalRouteHit,
                    "Resolved locally as system.open_app",
                    Some("No AI used".to_owned()),
                )?;
                let assistant = assistant_message(
                    &session.id,
                    content,
                    ChatOrigin::Local,
                    ChatRoute::LocalCapability,
                    started,
                    None,
                    None,
                    None,
                    None,
                );
                self.finish_chat(session, user_message, assistant, confirmation)
            }
            LocalIntent::RememberPreference { key, value } => {
                let receipt =
                    self.commit_preference(&key, &value, &format!("chat:{}", user_message.id))?;
                self.database
                    .add_usage(UsageStats {
                        local_resolutions: 1,
                        ..UsageStats::default()
                    })
                    .map_err(internal_error)?;
                self.activity(
                    Some(session.id.clone()),
                    correlation_id,
                    ActivityKind::MemoryCommitted,
                    "Explicit preference stored as versioned semantic memory",
                    Some(format!("{key} → {value}")),
                )?;
                let assistant = assistant_message(
                    &session.id,
                    format!("Remembered.\n{key} → {value}"),
                    ChatOrigin::Local,
                    ChatRoute::LocalMemory,
                    started,
                    None,
                    None,
                    None,
                    None,
                );
                let _ = receipt;
                self.finish_chat(session, user_message, assistant, None)
            }
            LocalIntent::ForgetPreference { key } => {
                let existing = self
                    .database
                    .preference_by_key(&key)
                    .map_err(internal_error)?;
                let content = if let Some((item, _)) = existing {
                    self.database
                        .set_memory_state(&item.id, None, Some(true), None)
                        .map_err(internal_error)?;
                    self.activity(
                        Some(session.id.clone()),
                        correlation_id,
                        ActivityKind::MemoryCommitted,
                        "Memory item disabled",
                        Some(key.clone()),
                    )?;
                    format!("Forgot the preference “{key}”.")
                } else {
                    format!("I couldn't find an active preference named “{key}”.")
                };
                self.database
                    .add_usage(UsageStats {
                        local_resolutions: 1,
                        ..UsageStats::default()
                    })
                    .map_err(internal_error)?;
                let assistant = assistant_message(
                    &session.id,
                    content,
                    ChatOrigin::Local,
                    ChatRoute::LocalMemory,
                    started,
                    None,
                    None,
                    None,
                    None,
                );
                self.finish_chat(session, user_message, assistant, None)
            }
            LocalIntent::ContextualOpenApp | LocalIntent::CorrectOpenApp { .. } => {
                Err(internal_error("contextual intent was not normalized"))
            }
        }
    }

    async fn ai_fallback(
        &mut self,
        selection: ModelSelection,
        session: halquen_domain::ChatSession,
        user_message: ChatMessage,
        correlation_id: &str,
        started: Instant,
        cancellation: &mut watch::Receiver<bool>,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let settings = self
            .database
            .application_settings()
            .map_err(internal_error)?;
        let providers = self.database.list_providers().map_err(internal_error)?;
        let models = self.database.list_models().map_err(internal_error)?;
        let selected = ModelRouter.select(
            &settings,
            &providers,
            &models,
            &RouteRequest {
                task: AiTaskType::Conversation,
                selection,
                contains_personal_context: settings.allow_personal_context,
            },
        );
        let selected = match selected {
            Ok(selected) => selected,
            Err(error) => {
                self.activity(
                    Some(session.id.clone()),
                    correlation_id,
                    ActivityKind::LocalRouteMiss,
                    "Request could not be resolved locally",
                    Some(error.to_string()),
                )?;
                let assistant = assistant_message(
                    &session.id,
                    "I couldn't resolve this locally and no eligible AI provider is configured."
                        .to_owned(),
                    ChatOrigin::System,
                    ChatRoute::Unavailable,
                    started,
                    None,
                    None,
                    None,
                    None,
                );
                return self.finish_chat(session, user_message, assistant, None);
            }
        };
        let provider = providers
            .iter()
            .find(|provider| provider.id == selected.provider_id)
            .ok_or_else(|| internal_error("selected provider disappeared"))?;
        let model = models
            .iter()
            .find(|model| model.id == selected.model_id)
            .ok_or_else(|| internal_error("selected model disappeared"))?;
        let credential = self.provider_credential(provider)?;
        let context = if settings.allow_personal_context {
            let history = self
                .database
                .list_chat_messages(&session.id, 8)
                .map_err(internal_error)?;
            history
                .into_iter()
                .filter(|message| message.id != user_message.id)
                .map(|message| ContextItem {
                    category: ContextCategory::RecentConversation,
                    content: format!("{:?}: {}", message.role, message.content),
                    priority: 10,
                    untrusted: message.origin != ChatOrigin::User,
                })
                .collect()
        } else {
            Vec::new()
        };
        let projection = ContextBuilder::new(settings.max_context_tokens).build(context);
        let has_retrieved_context = !projection.items.is_empty();
        let prompt = PromptComposer.compose(
            &PromptProfile {
                version: 1,
                task: AiTaskType::Conversation,
                task_instructions: "Answer the current conversational request. Do not propose or claim side effects unless a typed schema explicitly permits it.".to_owned(),
                output_schema: None,
            },
            &settings.personal_instructions,
        );
        self.activity(
            Some(session.id.clone()),
            correlation_id,
            ActivityKind::AiSelected,
            "AI fallback selected",
            Some(format!(
                "provider={} model={} privacy={:?} context≈{} tokens reason={}",
                provider.name,
                model.display_name,
                provider.privacy,
                projection.estimated_tokens,
                selected.reason_code
            )),
        )?;
        let request = AiRequest {
            task: AiTaskType::Conversation,
            system_prompt: prompt,
            user_message: user_message.content.clone(),
            context: projection.items,
            max_output_tokens: settings.max_output_tokens,
        };
        let response = if *cancellation.borrow() {
            None
        } else {
            let provider_call = self.provider_client.complete(
                provider,
                model,
                credential.as_deref().map(String::as_str),
                &request,
            );
            tokio::pin!(provider_call);
            tokio::select! {
                response = &mut provider_call => Some(response),
                _ = cancellation.changed() => {
                    let was_cancelled = *cancellation.borrow();
                    if was_cancelled {
                        None
                    } else {
                        Some(provider_call.await)
                    }
                }
            }
        };
        let Some(response) = response else {
            self.activity(
                Some(session.id.clone()),
                correlation_id,
                ActivityKind::AiFailed,
                "AI request cancelled",
                Some(
                    "The in-progress provider request was dropped without using its result"
                        .to_owned(),
                ),
            )?;
            let assistant = assistant_message(
                &session.id,
                "Request cancelled.".to_owned(),
                ChatOrigin::System,
                ChatRoute::Unavailable,
                started,
                None,
                None,
                None,
                None,
            );
            return self.finish_chat(session, user_message, assistant, None);
        };
        match response {
            Ok(response) => {
                let candidate_id = if has_retrieved_context {
                    None
                } else {
                    let created_at_ms = now_ms();
                    let candidate = CachedResponse {
                        id: CacheEntryId::generate(),
                        normalized_request: normalize_request(&user_message.content),
                        response: response.content.clone(),
                        context_key: "global".to_owned(),
                        confidence_permille: 550,
                        priority_permille: 500,
                        trust: TrustClass::AiInferred,
                        valid_until_ms: Some(
                            created_at_ms.saturating_add(RESPONSE_CANDIDATE_TTL_MS),
                        ),
                        reusable: false,
                        created_at_ms,
                        last_used_at_ms: None,
                        usage_count: 0,
                        success_count: 0,
                        correction_count: 0,
                        original_provider_id: Some(provider.id.clone()),
                        original_model_id: Some(model.id.clone()),
                        estimated_tokens_avoided: 0,
                    };
                    self.database
                        .store_response_candidate(&candidate)
                        .map_err(internal_error)?;
                    Some(candidate.id)
                };
                self.database
                    .add_usage(UsageStats {
                        model_requests: 1,
                        input_tokens: u64::from(response.usage.input_tokens),
                        output_tokens: u64::from(response.usage.output_tokens),
                        cached_tokens: u64::from(response.usage.cached_tokens),
                        ai_fallbacks: 1,
                        ..UsageStats::default()
                    })
                    .map_err(internal_error)?;
                self.activity(
                    Some(session.id.clone()),
                    correlation_id,
                    ActivityKind::AiCompleted,
                    "AI response completed",
                    Some(format!(
                        "input={} output={} cached={} reusable_candidate={}",
                        response.usage.input_tokens,
                        response.usage.output_tokens,
                        response.usage.cached_tokens,
                        candidate_id.is_some()
                    )),
                )?;
                let assistant = assistant_message(
                    &session.id,
                    response.content,
                    ChatOrigin::Ai,
                    ChatRoute::Ai,
                    started,
                    Some(provider.id.clone()),
                    Some(model.id.clone()),
                    Some((response.usage.input_tokens, response.usage.output_tokens)),
                    candidate_id,
                );
                self.finish_chat(session, user_message, assistant, None)
            }
            Err(error) => {
                self.database
                    .add_usage(UsageStats {
                        model_requests: 1,
                        ai_fallbacks: 1,
                        failed_provider_calls: 1,
                        ..UsageStats::default()
                    })
                    .map_err(internal_error)?;
                self.activity(
                    Some(session.id.clone()),
                    correlation_id,
                    ActivityKind::AiFailed,
                    "AI provider request failed",
                    Some(sanitized_ai_error(&error).to_owned()),
                )?;
                self.push_diagnostic(
                    DiagnosticSeverity::Warn,
                    "ai_gateway",
                    "provider_request_failed",
                    sanitized_ai_error(&error),
                    Some(correlation_id.to_owned()),
                );
                let assistant = assistant_message(
                    &session.id,
                    format!(
                        "AI is currently unavailable. {}",
                        sanitized_ai_error(&error)
                    ),
                    ChatOrigin::System,
                    ChatRoute::Unavailable,
                    started,
                    None,
                    None,
                    None,
                    None,
                );
                self.finish_chat(session, user_message, assistant, None)
            }
        }
    }

    fn finish_chat(
        &mut self,
        session: halquen_domain::ChatSession,
        user_message: ChatMessage,
        assistant_message: ChatMessage,
        confirmation: Option<ConfirmationPrompt>,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        self.database
            .append_chat_message(&assistant_message)
            .map_err(internal_error)?;
        Ok(ProtocolResponse::Chat {
            result: ChatResult {
                session,
                user_message,
                assistant_message,
                confirmation,
            },
        })
    }

    fn commit_preference(
        &mut self,
        key: &str,
        value: &str,
        source_reference: &str,
    ) -> Result<MemoryMutationReceipt, ProtocolErrorBody> {
        let timestamp = now_ms();
        let evidence = Evidence {
            id: EvidenceId::generate(),
            trust: TrustClass::UserExplicit,
            source_reference: Some(source_reference.to_owned()),
            created_at_ms: timestamp,
        };
        let memory_value = MemoryValue::Preference {
            key: key.to_owned(),
            value: value.to_owned(),
        };
        let existing = self
            .database
            .preference_by_key(key)
            .map_err(internal_error)?;
        let (item, revision) = if let Some((mut item, _)) = existing {
            let revision = MemoryRevision {
                id: MemoryRevisionId::generate(),
                memory_id: item.id.clone(),
                previous_revision_id: Some(item.current_revision_id.clone()),
                value: memory_value,
                evidence_ids: vec![evidence.id.clone()],
                created_at_ms: timestamp,
                valid_from_ms: Some(timestamp),
                valid_until_ms: None,
            };
            item.current_revision_id = revision.id.clone();
            item.updated_at_ms = timestamp;
            (item, revision)
        } else {
            let memory_id = MemoryId::generate();
            let revision = MemoryRevision {
                id: MemoryRevisionId::generate(),
                memory_id: memory_id.clone(),
                previous_revision_id: None,
                value: memory_value,
                evidence_ids: vec![evidence.id.clone()],
                created_at_ms: timestamp,
                valid_from_ms: Some(timestamp),
                valid_until_ms: None,
            };
            let item = MemoryItem {
                id: memory_id,
                kind: MemoryKind::Semantic,
                current_revision_id: revision.id.clone(),
                created_at_ms: timestamp,
                updated_at_ms: timestamp,
            };
            (item, revision)
        };
        self.database
            .persist_memory_revision(&item, &revision, &[evidence])
            .map_err(internal_error)?;
        Ok(MemoryMutationReceipt {
            memory_id: item.id,
            revision_id: revision.id,
            summary: format!("{key} → {value}"),
        })
    }

    pub(crate) fn list_chat_sessions(
        &self,
        limit: u16,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        Ok(ProtocolResponse::ChatSessions {
            sessions: self
                .database
                .list_chat_sessions(limit)
                .map_err(internal_error)?,
        })
    }

    pub(crate) fn list_chat_messages(
        &self,
        session_id: &halquen_domain::ChatSessionId,
        limit: u16,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        Ok(ProtocolResponse::ChatMessages {
            messages: self
                .database
                .list_chat_messages(session_id, limit)
                .map_err(internal_error)?,
        })
    }

    pub(crate) fn list_activity(&self, limit: u16) -> Result<ProtocolResponse, ProtocolErrorBody> {
        Ok(ProtocolResponse::Activity {
            events: self.database.list_activity(limit).map_err(internal_error)?,
        })
    }

    pub(crate) fn list_memory(
        &self,
        query: MemoryQuery,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        if query.search.as_ref().is_some_and(|value| value.len() > 512) {
            return Err(validation("memory search is too long"));
        }
        Ok(ProtocolResponse::MemoryItems {
            items: self
                .database
                .list_memory(query.kind, query.search.as_deref(), query.limit)
                .map_err(internal_error)?,
        })
    }

    pub(crate) fn memory_history(
        &self,
        memory_id: &MemoryId,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        Ok(ProtocolResponse::MemoryHistory {
            revisions: self
                .database
                .memory_history(memory_id)
                .map_err(internal_error)?,
        })
    }

    pub(crate) fn update_memory_state(
        &mut self,
        update: MemoryStateUpdate,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let updated = self
            .database
            .set_memory_state(
                &update.memory_id,
                update.pinned,
                update.disabled,
                update.priority_permille,
            )
            .map_err(internal_error)?;
        Ok(ProtocolResponse::MemoryUpdated { updated })
    }

    pub(crate) fn restore_memory_revision(
        &mut self,
        memory_id: &MemoryId,
        revision_id: &MemoryRevisionId,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let (mut item, _) = self
            .database
            .memory_head(memory_id)
            .map_err(internal_error)?
            .ok_or_else(not_found)?;
        let source = self
            .database
            .memory_revision(memory_id, revision_id)
            .map_err(internal_error)?
            .ok_or_else(not_found)?;
        let timestamp = now_ms();
        let evidence = Evidence {
            id: EvidenceId::generate(),
            trust: TrustClass::UserExplicit,
            source_reference: Some(format!("restore:{}", revision_id)),
            created_at_ms: timestamp,
        };
        let revision = MemoryRevision {
            id: MemoryRevisionId::generate(),
            memory_id: memory_id.clone(),
            previous_revision_id: Some(item.current_revision_id.clone()),
            value: source.value,
            evidence_ids: vec![evidence.id.clone()],
            created_at_ms: timestamp,
            valid_from_ms: Some(timestamp),
            valid_until_ms: None,
        };
        item.current_revision_id = revision.id.clone();
        item.updated_at_ms = timestamp;
        self.database
            .persist_memory_revision(&item, &revision, &[evidence])
            .map_err(internal_error)?;
        Ok(ProtocolResponse::MemoryMutation {
            receipt: MemoryMutationReceipt {
                memory_id: item.id,
                revision_id: revision.id,
                summary: "Restored as a new revision".to_owned(),
            },
        })
    }

    pub(crate) fn list_providers(&self) -> Result<ProtocolResponse, ProtocolErrorBody> {
        Ok(ProtocolResponse::Providers {
            providers: self.database.list_providers().map_err(internal_error)?,
        })
    }

    pub(crate) fn upsert_provider(
        &mut self,
        input: ProviderUpsert,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        if input
            .api_key
            .as_ref()
            .is_some_and(|value| value.is_empty() || value.len() > 8_192 || value.contains('\0'))
        {
            return Err(validation("provider credential is empty or too large"));
        }
        let timestamp = now_ms();
        let id = input.id.unwrap_or_else(ProviderId::generate);
        let previous = self.database.provider(&id).map_err(internal_error)?;
        let created_at_ms = previous
            .as_ref()
            .map_or(timestamp, |provider| provider.created_at_ms);
        let previous_credential_id = previous
            .as_ref()
            .and_then(|provider| provider.credential_id.clone());
        let api_key = input.api_key.map(Zeroizing::new);
        let replacement_reference = previous_credential_id
            .clone()
            .unwrap_or_else(|| format!("provider:{}", id));
        let credential_id = if api_key.is_some() {
            Some(replacement_reference.clone())
        } else if input.clear_api_key {
            None
        } else {
            previous_credential_id.clone()
        };
        let mut provider = Provider {
            id,
            kind: input.kind,
            name: input.name,
            base_url: input.base_url,
            enabled: input.enabled,
            privacy: input.privacy,
            configured: credential_id.is_some() || input.privacy == PrivacyClass::Local,
            credential_id,
            status: ProviderStatus::Configured,
            created_at_ms,
            updated_at_ms: timestamp,
        };
        match validate_provider(&provider) {
            Ok(()) => {}
            Err(AiError::UnsupportedProvider) => provider.status = ProviderStatus::Unsupported,
            Err(_) => return Err(validation("provider endpoint is not permitted")),
        }

        let mutation_reference = if api_key.is_some() {
            Some(replacement_reference)
        } else if input.clear_api_key {
            previous_credential_id
        } else {
            None
        };
        let previous_secret = mutation_reference
            .as_deref()
            .map(|reference| capture_secret(&*self.secret_store, reference))
            .transpose()?
            .flatten();

        if let (Some(reference), Some(secret)) = (mutation_reference.as_deref(), api_key) {
            self.secret_store
                .store(reference, secret)
                .map_err(|_| secret_store_error())?;
        } else if let Some(reference) = mutation_reference.as_deref() {
            match self.secret_store.delete(reference) {
                Ok(()) | Err(SecretError::NotFound) => {}
                Err(_) => return Err(secret_store_error()),
            }
        }

        if let Err(error) = self.database.upsert_provider(&provider) {
            if let Some(reference) = mutation_reference.as_deref() {
                restore_secret(&*self.secret_store, reference, previous_secret)?;
            }
            return Err(internal_error(error));
        }
        Ok(ProtocolResponse::ProviderSaved { provider })
    }

    pub(crate) fn remove_provider(
        &mut self,
        provider_id: &ProviderId,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let provider = self
            .database
            .provider(provider_id)
            .map_err(internal_error)?;
        let credential_id = provider.and_then(|provider| provider.credential_id);
        let previous_secret = credential_id
            .as_deref()
            .map(|reference| capture_secret(&*self.secret_store, reference))
            .transpose()?
            .flatten();
        if let Some(reference) = credential_id.as_deref() {
            match self.secret_store.delete(reference) {
                Ok(()) | Err(SecretError::NotFound) => {}
                Err(_) => return Err(secret_store_error()),
            }
        }
        let removed = match self.database.remove_provider(provider_id) {
            Ok(removed) => removed,
            Err(error) => {
                if let Some(reference) = credential_id.as_deref() {
                    restore_secret(&*self.secret_store, reference, previous_secret)?;
                }
                return Err(internal_error(error));
            }
        };
        if !removed && let Some(reference) = credential_id.as_deref() {
            restore_secret(&*self.secret_store, reference, previous_secret)?;
        }
        Ok(ProtocolResponse::ProviderRemoved { removed })
    }

    pub(crate) async fn test_provider(
        &mut self,
        provider_id: &ProviderId,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let mut provider = self
            .database
            .provider(provider_id)
            .map_err(internal_error)?
            .ok_or_else(not_found)?;
        let credential = self.provider_credential(&provider)?;
        let result = self
            .provider_client
            .test(&provider, credential.as_deref().map(String::as_str))
            .await;
        let (status, message) = match result {
            Ok(result) => (ProviderStatus::Connected, result.sanitized_message),
            Err(error) => (
                provider_status_for_error(&error),
                sanitized_ai_error(&error).to_owned(),
            ),
        };
        provider.status = status;
        provider.updated_at_ms = now_ms();
        self.database
            .upsert_provider(&provider)
            .map_err(internal_error)?;
        Ok(ProtocolResponse::ProviderTest {
            result: ProviderTestStatus {
                provider_id: provider.id,
                status,
                message,
            },
        })
    }

    fn provider_credential(
        &self,
        provider: &Provider,
    ) -> Result<Option<Zeroizing<String>>, ProtocolErrorBody> {
        provider
            .credential_id
            .as_deref()
            .map(|reference| {
                self.secret_store
                    .retrieve(reference)
                    .map_err(|_| secret_store_error())
            })
            .transpose()
    }

    pub(crate) fn list_models(&self) -> Result<ProtocolResponse, ProtocolErrorBody> {
        Ok(ProtocolResponse::Models {
            models: self.database.list_models().map_err(internal_error)?,
        })
    }

    pub(crate) fn upsert_model(
        &mut self,
        input: ModelUpsert,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let model = input.into_model();
        let provider = self
            .database
            .provider(&model.provider_id)
            .map_err(internal_error)?
            .ok_or_else(not_found)?;
        if provider.privacy != model.privacy {
            return Err(validation("model privacy must match its provider"));
        }
        self.database.upsert_model(&model).map_err(internal_error)?;
        Ok(ProtocolResponse::ModelSaved { model })
    }

    pub(crate) fn application_settings(&self) -> Result<ProtocolResponse, ProtocolErrorBody> {
        Ok(ProtocolResponse::ApplicationSettings {
            settings: self
                .database
                .application_settings()
                .map_err(internal_error)?,
        })
    }

    pub(crate) fn update_application_settings(
        &mut self,
        settings: halquen_domain::ApplicationSettings,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        settings
            .validate()
            .map_err(|error| validation(&error.to_string()))?;
        self.database
            .update_application_settings(&settings, now_ms())
            .map_err(internal_error)?;
        Ok(ProtocolResponse::SettingsUpdated { settings })
    }

    pub(crate) fn usage_stats(&self) -> Result<ProtocolResponse, ProtocolErrorBody> {
        Ok(ProtocolResponse::UsageStats {
            stats: self.database.usage_stats().map_err(internal_error)?,
        })
    }

    pub(crate) fn diagnostics(&self, limit: u16) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let memory = self.database.memory_stats().map_err(internal_error)?;
        let audit = self.database.audit_stats().map_err(internal_error)?;
        let providers = self.database.list_providers().map_err(internal_error)?;
        let recent = self
            .diagnostics
            .iter()
            .rev()
            .take(usize::from(limit.clamp(1, 200)))
            .cloned()
            .collect();
        Ok(ProtocolResponse::Diagnostics {
            snapshot: DiagnosticsSnapshot {
                protocol_version: PROTOCOL_VERSION,
                schema_version: self.database.schema_version().map_err(internal_error)?,
                database_path: self.environment.database_path.clone(),
                runtime_socket: self.environment.runtime_socket.clone(),
                provider_statuses: providers
                    .into_iter()
                    .map(|provider| ProviderTestStatus {
                        provider_id: provider.id,
                        status: provider.status,
                        message: "Last known status".to_owned(),
                    })
                    .collect(),
                recent,
                memory_items: memory.items,
                cached_responses: self
                    .database
                    .cached_response_count()
                    .map_err(internal_error)?,
                unknown_cases: memory.unknown_cases,
                audit_records: audit.records,
            },
        })
    }

    pub(crate) fn clear_operational_logs(&mut self) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let removed = crate::logging::clear_historical_logs().map_err(internal_error)?;
        self.diagnostics.clear();
        Ok(ProtocolResponse::OperationalLogsCleared { removed })
    }

    pub(crate) fn submit_response_feedback(
        &mut self,
        cache_entry_id: &CacheEntryId,
        feedback: ResponseFeedback,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        self.database
            .apply_response_feedback(cache_entry_id, feedback)
            .map_err(internal_error)?;
        Ok(ProtocolResponse::FeedbackRecorded)
    }

    pub(crate) fn preview_ai_request(
        &self,
        request: ChatRequest,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        if request.message.trim().is_empty() || request.message.len() > 16_384 {
            return Err(validation("message must contain between 1 and 16384 bytes"));
        }
        let settings = self
            .database
            .application_settings()
            .map_err(internal_error)?;
        let providers = self.database.list_providers().map_err(internal_error)?;
        let models = self.database.list_models().map_err(internal_error)?;
        let contains_personal_context =
            settings.allow_personal_context && request.session_id.is_some();
        let selected = ModelRouter
            .select(
                &settings,
                &providers,
                &models,
                &RouteRequest {
                    task: AiTaskType::Conversation,
                    selection: request.model_selection,
                    contains_personal_context,
                },
            )
            .ok();
        let context = if settings.allow_personal_context {
            request
                .session_id
                .as_ref()
                .map(|session_id| self.database.list_chat_messages(session_id, 8))
                .transpose()
                .map_err(internal_error)?
                .unwrap_or_default()
                .into_iter()
                .map(|message| ContextItem {
                    category: ContextCategory::RecentConversation,
                    content: format!("{:?}: {}", message.role, message.content),
                    priority: 10,
                    untrusted: message.origin != ChatOrigin::User,
                })
                .collect()
        } else {
            Vec::new()
        };
        let projection = ContextBuilder::new(settings.max_context_tokens).build(context);
        let current_request_tokens =
            u32::try_from(request.message.chars().count().div_ceil(4)).unwrap_or(u32::MAX);
        let mut categories = vec![ContextCategory::CurrentRequest];
        if !projection.items.is_empty() {
            categories.push(ContextCategory::RecentConversation);
        }
        Ok(ProtocolResponse::AiRequestPreview {
            preview: PromptPreview {
                provider_id: selected
                    .as_ref()
                    .map(|selected| selected.provider_id.clone()),
                model_id: selected.map(|selected| selected.model_id),
                task: AiTaskType::Conversation,
                estimated_context_tokens: current_request_tokens
                    .saturating_add(projection.estimated_tokens),
                context_categories: categories,
                personal_instructions: settings.personal_instructions,
                core_contract_managed: true,
            },
        })
    }

    pub(crate) async fn confirm_action(
        &mut self,
        confirmation_id: &str,
        allow: bool,
        persistence: ConfirmationPersistence,
        expires_at_ms: Option<i64>,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let pending = self
            .pending_confirmations
            .remove(confirmation_id)
            .ok_or_else(confirmation_expired)?;
        if pending.expires_at_ms < now_ms() {
            return Err(confirmation_expired());
        }
        if !allow {
            self.database
                .append_audit(&AuditRecord {
                    id: AuditId::generate(),
                    created_at_ms: now_ms(),
                    event: AuditEvent::ConfirmationReceived {
                        execution_id: pending.request_execution_id,
                        capability_id: pending.proposal.action.capability_id,
                        accepted: false,
                        agent: pending.proposal.context.agent,
                    },
                })
                .map_err(internal_error)?;
            return Ok(ProtocolResponse::Confirmation {
                result: ConfirmationResult {
                    execution_id: None,
                    accepted: false,
                    message: "Action cancelled".to_owned(),
                },
            });
        }
        let descriptor = self.descriptor_for(&pending.proposal.action)?.clone();
        let execution_id = ExecutionId::generate();
        let proposal_id = ProposalId::generate();
        let started_at_ms = now_ms();
        let context = self.policy_context(&pending.proposal, pending.session_id.as_ref())?;
        let evaluation = self.policy.authorize_confirmed_proposal_once(
            &descriptor,
            pending.proposal.clone(),
            execution_id.clone(),
            &context,
        );
        let decision = evaluation.decision.clone();
        let authorization = evaluation
            .into_authorization()
            .ok_or_else(|| ProtocolErrorBody {
                code: ProtocolErrorCode::PrivacyDenied,
                message: "policy denied the confirmed action".to_owned(),
            })?;
        if persistence != ConfirmationPersistence::Once {
            let (lifetime, session, expiry) = match persistence {
                ConfirmationPersistence::Once => {
                    return Err(validation("once confirmation does not create a grant"));
                }
                ConfirmationPersistence::Session => {
                    let session = if let Some(identity) = &pending.proposal.context.agent {
                        PermissionSessionScope::Agent(identity.session_id.clone())
                    } else {
                        PermissionSessionScope::Chat(
                            pending
                                .session_id
                                .clone()
                                .ok_or_else(|| validation("session confirmation has no session"))?,
                        )
                    };
                    (PermissionLifetime::Session, Some(session), None)
                }
                ConfirmationPersistence::Until => {
                    let expiry = expires_at_ms
                        .filter(|expiry| *expiry > started_at_ms)
                        .ok_or_else(|| {
                            validation("confirmation expiration must be in the future")
                        })?;
                    (PermissionLifetime::Until, None, Some(expiry))
                }
                ConfirmationPersistence::Always => (PermissionLifetime::Always, None, None),
            };
            self.database
                .upsert_permission_grant(&PermissionGrant {
                    id: PermissionId::generate(),
                    effect: PermissionEffect::Allow,
                    lifetime,
                    scope: PermissionScope::from_proposal(&pending.proposal),
                    session,
                    agent_id: pending
                        .proposal
                        .context
                        .agent
                        .as_ref()
                        .map(|identity| identity.agent_id.clone()),
                    granted_by: ActionOrigin::UserExplicit,
                    granted_at_ms: started_at_ms,
                    expires_at_ms: expiry,
                    revoked_at_ms: None,
                    use_limit: None,
                    use_count: 0,
                })
                .map_err(internal_error)?;
        }
        let mut audit_records = vec![
            AuditRecord {
                id: AuditId::generate(),
                created_at_ms: started_at_ms,
                event: AuditEvent::ProposalCreated {
                    execution_id: execution_id.clone(),
                    proposal_id,
                    capability_id: descriptor.id.clone(),
                    context: pending.proposal.context.sanitized_summary(),
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
            AuditRecord {
                id: AuditId::generate(),
                created_at_ms: started_at_ms,
                event: AuditEvent::ConfirmationReceived {
                    execution_id: execution_id.clone(),
                    capability_id: descriptor.id.clone(),
                    accepted: true,
                    agent: pending.proposal.context.agent.clone(),
                },
            },
            AuditRecord {
                id: AuditId::generate(),
                created_at_ms: started_at_ms,
                event: AuditEvent::AuthorizationCreated {
                    execution_id: execution_id.clone(),
                    capability_id: descriptor.id.clone(),
                    agent: pending.proposal.context.agent.clone(),
                },
            },
            AuditRecord {
                id: AuditId::generate(),
                created_at_ms: started_at_ms,
                event: AuditEvent::ExecutionStarted {
                    execution_id: execution_id.clone(),
                    capability_id: descriptor.id.clone(),
                },
            },
        ];
        let (status, result_code, error_code, sanitized_error) = match timeout(
            Duration::from_millis(descriptor.timeout_ms),
            self.executor.execute(authorization),
        )
        .await
        {
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
                let code = crate::service::execution_error_code(&error).to_owned();
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
                    Some("executor rejected the confirmed request".to_owned()),
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
                    Some("confirmed capability exceeded its deadline".to_owned()),
                )
            }
        };
        if matches!(
            status,
            ExecutionStatus::DryRunSucceeded | ExecutionStatus::Succeeded
        ) && let ActionArguments::OpenApp { app } = &pending.proposal.action.arguments
        {
            self.database
                .record_intent_usage(&IntentUsageEvent {
                    id: BehaviourEventId::generate(),
                    intent: "open_application".to_owned(),
                    entity_id: app.clone(),
                    outcome: BehaviourOutcome::Success,
                    context_class: "application".to_owned(),
                    created_at_ms: now_ms(),
                })
                .map_err(internal_error)?;
        }
        let receipt = ExecutionReceipt {
            execution_id: execution_id.clone(),
            capability_id: descriptor.id,
            capability_version: descriptor.version,
            started_at_ms,
            finished_at_ms: now_ms().max(started_at_ms),
            policy_decision: decision,
            status,
            reversible: descriptor.reversible,
            result_code,
            error_code,
            sanitized_error,
            compensation_reference: None,
        };
        self.database
            .record_execution(&receipt, &audit_records)
            .map_err(internal_error)?;
        Ok(ProtocolResponse::Confirmation {
            result: ConfirmationResult {
                execution_id: Some(execution_id),
                accepted: true,
                message: if status == ExecutionStatus::DryRunSucceeded {
                    format!(
                        "{}: confirmed dry-run completed; no real side effect was performed",
                        pending.title
                    )
                } else {
                    format!("{}: confirmed execution completed", pending.title)
                },
            },
        })
    }

    fn activity(
        &mut self,
        session_id: Option<halquen_domain::ChatSessionId>,
        correlation_id: &str,
        kind: ActivityKind,
        summary: &str,
        detail: Option<String>,
    ) -> Result<(), ProtocolErrorBody> {
        self.database
            .append_activity(&ActivityEvent {
                id: ActivityId::generate(),
                session_id,
                correlation_id: correlation_id.to_owned(),
                kind,
                summary: summary.to_owned(),
                detail,
                created_at_ms: now_ms(),
            })
            .map_err(internal_error)
    }

    fn push_diagnostic(
        &mut self,
        severity: DiagnosticSeverity,
        component: &str,
        code: &str,
        message: &str,
        correlation_id: Option<String>,
    ) {
        if self.diagnostics.len() == 200 {
            self.diagnostics.pop_front();
        }
        self.diagnostics.push_back(DiagnosticEntry {
            timestamp_ms: now_ms(),
            severity,
            component: component.to_owned(),
            code: code.to_owned(),
            message: message.to_owned(),
            correlation_id,
        });
    }
}

// A single constructor keeps every persisted assistant message on the same metadata path.
#[allow(clippy::too_many_arguments)]
fn assistant_message(
    session_id: &halquen_domain::ChatSessionId,
    content: String,
    origin: ChatOrigin,
    route: ChatRoute,
    started: Instant,
    provider_id: Option<ProviderId>,
    model_id: Option<halquen_domain::ModelId>,
    usage: Option<(u32, u32)>,
    reusable_candidate_id: Option<CacheEntryId>,
) -> ChatMessage {
    ChatMessage {
        id: ChatMessageId::generate(),
        session_id: session_id.clone(),
        role: ChatRole::Assistant,
        content,
        origin,
        route: Some(route),
        provider_id,
        model_id,
        input_tokens: usage.map(|usage| usage.0),
        output_tokens: usage.map(|usage| usage.1),
        latency_ms: Some(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)),
        reusable_candidate_id,
        created_at_ms: now_ms(),
    }
}

fn display_for_entity(entity: &halquen_domain::EntityId) -> String {
    entity
        .as_str()
        .strip_prefix("app:")
        .unwrap_or(entity.as_str())
        .replace('_', " ")
}

fn validation(message: &str) -> ProtocolErrorBody {
    ProtocolErrorBody {
        code: ProtocolErrorCode::Validation,
        message: message.to_owned(),
    }
}

fn not_found() -> ProtocolErrorBody {
    ProtocolErrorBody {
        code: ProtocolErrorCode::NotFound,
        message: "requested item was not found".to_owned(),
    }
}

fn confirmation_expired() -> ProtocolErrorBody {
    ProtocolErrorBody {
        code: ProtocolErrorCode::ConfirmationExpired,
        message: "confirmation was already used, cancelled or expired".to_owned(),
    }
}

fn secret_store_error() -> ProtocolErrorBody {
    ProtocolErrorBody {
        code: ProtocolErrorCode::SecretStoreUnavailable,
        message: "OS credential storage is unavailable; the secret was not stored".to_owned(),
    }
}

fn capture_secret(
    store: &dyn SecretStore,
    credential_id: &str,
) -> Result<Option<Zeroizing<String>>, ProtocolErrorBody> {
    match store.retrieve(credential_id) {
        Ok(secret) => Ok(Some(secret)),
        Err(SecretError::NotFound) => Ok(None),
        Err(_) => Err(secret_store_error()),
    }
}

fn restore_secret(
    store: &dyn SecretStore,
    credential_id: &str,
    previous: Option<Zeroizing<String>>,
) -> Result<(), ProtocolErrorBody> {
    match previous {
        Some(secret) => store
            .store(credential_id, secret)
            .map_err(|_| secret_store_error()),
        None => match store.delete(credential_id) {
            Ok(()) | Err(SecretError::NotFound) => Ok(()),
            Err(_) => Err(secret_store_error()),
        },
    }
}

fn sanitized_ai_error(error: &AiError) -> &'static str {
    match error {
        AiError::Disabled => "AI is disabled by settings.",
        AiError::NoEligibleRoute => "No eligible provider/model route is configured.",
        AiError::UnsupportedProvider => "This provider adapter is not implemented yet.",
        AiError::InvalidEndpoint => "The provider endpoint is invalid.",
        AiError::CredentialUnavailable => "The provider credential is unavailable.",
        AiError::AuthenticationFailed => "Provider authentication failed.",
        AiError::RateLimited => "The provider rate limit was reached.",
        AiError::EndpointUnavailable => "The provider endpoint is unreachable.",
        AiError::InvalidResponse => "The provider returned an invalid response.",
        AiError::ResponseTooLarge => "The provider response exceeded the safe size limit.",
        AiError::RequestFailed => "The provider request failed.",
    }
}

fn provider_status_for_error(error: &AiError) -> ProviderStatus {
    match error {
        AiError::AuthenticationFailed | AiError::CredentialUnavailable => {
            ProviderStatus::AuthenticationFailed
        }
        AiError::RateLimited => ProviderStatus::RateLimited,
        AiError::UnsupportedProvider => ProviderStatus::Unsupported,
        AiError::EndpointUnavailable | AiError::InvalidEndpoint => {
            ProviderStatus::EndpointUnreachable
        }
        _ => ProviderStatus::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};

    use halquen_ai::{AgentHost, AiResponse, AiUsage, ProviderFuture, ProviderTestResult};
    use halquen_capabilities::{DryRunExecutor, inspect_executable};
    use halquen_domain::{
        AgentConfiguration, AgentResourceLimits, AgentTransport, AiModel, ExecutableOwnership,
        ModelId, ProviderKind, SandboxBackend,
    };
    use halquen_protocol::{AgentProposalDisposition, AgentRunRequest};

    use super::*;

    #[derive(Default)]
    struct FakeProviderClient {
        calls: AtomicU32,
    }

    #[derive(Default)]
    struct SlowProviderClient {
        calls: AtomicU32,
    }

    impl ProviderClient for FakeProviderClient {
        fn complete<'a>(
            &'a self,
            _provider: &'a Provider,
            model: &'a AiModel,
            _credential: Option<&'a str>,
            _request: &'a AiRequest,
        ) -> ProviderFuture<'a, AiResponse> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Box::pin(async move {
                Ok(AiResponse {
                    content: "A controlled fake provider response.".to_owned(),
                    provider_model_id: model.provider_model_id.clone(),
                    usage: AiUsage {
                        input_tokens: 12,
                        output_tokens: 7,
                        cached_tokens: 0,
                        reasoning_tokens: 0,
                    },
                })
            })
        }

        fn test<'a>(
            &'a self,
            _provider: &'a Provider,
            _credential: Option<&'a str>,
        ) -> ProviderFuture<'a, ProviderTestResult> {
            Box::pin(async {
                Ok(ProviderTestResult {
                    reachable: true,
                    sanitized_message: "Connected".to_owned(),
                })
            })
        }
    }

    impl ProviderClient for SlowProviderClient {
        fn complete<'a>(
            &'a self,
            _provider: &'a Provider,
            model: &'a AiModel,
            _credential: Option<&'a str>,
            _request: &'a AiRequest,
        ) -> ProviderFuture<'a, AiResponse> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Box::pin(async move {
                tokio::time::sleep(Duration::from_secs(60)).await;
                Ok(AiResponse {
                    content: "This response must never be used after cancellation.".to_owned(),
                    provider_model_id: model.provider_model_id.clone(),
                    usage: AiUsage::default(),
                })
            })
        }

        fn test<'a>(
            &'a self,
            _provider: &'a Provider,
            _credential: Option<&'a str>,
        ) -> ProviderFuture<'a, ProviderTestResult> {
            Box::pin(async {
                Ok(ProviderTestResult {
                    reachable: true,
                    sanitized_message: "Connected".to_owned(),
                })
            })
        }
    }

    #[derive(Default)]
    struct FakeSecretStore {
        values: Mutex<BTreeMap<String, String>>,
    }

    impl SecretStore for FakeSecretStore {
        fn store(&self, credential_id: &str, secret: Zeroizing<String>) -> Result<(), SecretError> {
            self.values
                .lock()
                .unwrap()
                .insert(credential_id.to_owned(), secret.to_string());
            Ok(())
        }

        fn retrieve(&self, credential_id: &str) -> Result<Zeroizing<String>, SecretError> {
            self.values
                .lock()
                .unwrap()
                .get(credential_id)
                .cloned()
                .map(Zeroizing::new)
                .ok_or(SecretError::NotFound)
        }

        fn delete(&self, credential_id: &str) -> Result<(), SecretError> {
            self.values.lock().unwrap().remove(credential_id);
            Ok(())
        }
    }

    fn service() -> HalquenService<DryRunExecutor> {
        HalquenService::new(
            DryRunExecutor::new(),
            halquen_storage::Database::open_in_memory().unwrap(),
        )
        .unwrap()
    }

    fn chat(message: &str) -> ChatRequest {
        ChatRequest {
            session_id: None,
            message: message.to_owned(),
            model_selection: ModelSelection::Automatic,
        }
    }

    #[tokio::test]
    async fn local_open_app_does_not_call_ai() {
        let fake = Arc::new(FakeProviderClient::default());
        let mut service = service();
        service.set_integrations(fake.clone(), Arc::new(FakeSecretStore::default()));
        let response = service
            .chat(chat("Open Telegram"), "request:local")
            .await
            .unwrap();
        let result = match response {
            ProtocolResponse::Chat { result } => result,
            other => panic!("unexpected response: {other:?}"),
        };
        assert_eq!(
            result.assistant_message.route,
            Some(ChatRoute::LocalCapability)
        );
        assert!(
            result
                .assistant_message
                .content
                .contains("No application was launched")
        );
        assert_eq!(fake.calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn recent_context_resolves_safe_ambiguous_open_locally() {
        let mut service = service();
        for index in 0..3 {
            service
                .chat(chat("Open Discord"), &format!("request:discord:{index}"))
                .await
                .unwrap();
        }
        let response = service
            .chat(chat("запусти тот мессенджер"), "request:contextual")
            .await
            .unwrap();
        match response {
            ProtocolResponse::Chat { result } => {
                assert_eq!(
                    result.assistant_message.route,
                    Some(ChatRoute::LocalCapability)
                );
                assert!(result.assistant_message.content.contains("discord"));
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[tokio::test]
    async fn close_recent_scores_request_clarification() {
        let mut service = service();
        service
            .chat(chat("Open Telegram"), "request:telegram")
            .await
            .unwrap();
        service
            .chat(chat("Open Discord"), "request:discord")
            .await
            .unwrap();
        let response = service
            .chat(chat("open that messenger"), "request:ambiguous")
            .await
            .unwrap();
        match response {
            ProtocolResponse::Chat { result } => {
                assert_eq!(
                    result.assistant_message.route,
                    Some(ChatRoute::Clarification)
                );
                assert!(result.confirmation.is_none());
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[tokio::test]
    async fn explicit_correction_changes_future_ranking_without_granting_permission() {
        let mut service = service();
        service
            .chat(chat("Open Telegram"), "request:telegram")
            .await
            .unwrap();
        service
            .chat(chat("не Telegram, Discord"), "request:correction")
            .await
            .unwrap();
        assert!(
            service
                .database
                .list_permission_grants(10)
                .unwrap()
                .is_empty()
        );
        let response = service
            .chat(chat("open that messenger"), "request:after-correction")
            .await
            .unwrap();
        match response {
            ProtocolResponse::Chat { result } => {
                assert!(result.assistant_message.content.contains("discord"));
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[tokio::test]
    async fn confirmed_exact_permission_persists_and_revocation_restores_confirmation() {
        let mut service = service();
        service
            .database
            .update_security_profile(halquen_domain::SecurityProfile::Strict, now_ms())
            .unwrap();
        let first = service
            .chat(chat("Open Telegram"), "request:strict:first")
            .await
            .unwrap();
        let confirmation_id = match first {
            ProtocolResponse::Chat { result } => result.confirmation.unwrap().confirmation_id,
            other => panic!("unexpected response: {other:?}"),
        };
        service
            .confirm_action(
                &confirmation_id,
                true,
                ConfirmationPersistence::Always,
                None,
            )
            .await
            .unwrap();
        let grants = service.database.list_permission_grants(10).unwrap();
        assert_eq!(grants.len(), 1);

        let second = service
            .chat(chat("Open Telegram"), "request:strict:second")
            .await
            .unwrap();
        match second {
            ProtocolResponse::Chat { result } => assert!(result.confirmation.is_none()),
            other => panic!("unexpected response: {other:?}"),
        }

        service
            .database
            .revoke_permission(&grants[0].id, now_ms())
            .unwrap();
        let third = service
            .chat(chat("Open Telegram"), "request:strict:third")
            .await
            .unwrap();
        match third {
            ProtocolResponse::Chat { result } => assert!(result.confirmation.is_some()),
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[tokio::test]
    async fn confirmation_token_is_single_use_and_replay_has_no_side_effects() {
        let mut service = service();
        service
            .database
            .update_security_profile(halquen_domain::SecurityProfile::Strict, now_ms())
            .unwrap();
        let response = service
            .chat(chat("Open Telegram"), "request:strict:single-use")
            .await
            .unwrap();
        let confirmation_id = match response {
            ProtocolResponse::Chat { result } => result.confirmation.unwrap().confirmation_id,
            other => panic!("unexpected response: {other:?}"),
        };

        service
            .confirm_action(&confirmation_id, true, ConfirmationPersistence::Once, None)
            .await
            .unwrap();
        let after_first = service.database.audit_stats().unwrap();

        let replay = service
            .confirm_action(&confirmation_id, true, ConfirmationPersistence::Once, None)
            .await
            .unwrap_err();
        assert_eq!(replay.code, ProtocolErrorCode::ConfirmationExpired);
        assert_eq!(service.database.audit_stats().unwrap(), after_first);
    }

    #[tokio::test]
    async fn brokered_agent_unknown_capability_is_rejected_without_execution() {
        let executable = match Path::new("/usr/bin/python3").canonicalize() {
            Ok(executable) => executable,
            Err(_) => return,
        };
        let Some(executable) = executable.to_str().map(str::to_owned) else {
            return;
        };
        let Ok(identity) = inspect_executable(&executable, ExecutableOwnership::RootOnly, None)
        else {
            return;
        };
        let source = r#"import sys,json
json.loads(sys.stdin.readline())
print(json.dumps({"version":1,"kind":"proposals","message":"unknown","proposals":[{"action":{"capability_id":"system.not_registered","arguments":{"kind":"open_app","app":"app:test"}},"explanation":"test"}]}), flush=True)
result=json.loads(sys.stdin.readline())
assert result["results"][0]["disposition"] == "failed"
"#;
        let agent = AgentConfiguration {
            id: halquen_domain::AgentId::generate(),
            name: "unknown capability fixture".to_owned(),
            transport: AgentTransport::Cli,
            executable,
            arguments: vec!["-c".to_owned(), source.to_owned()],
            socket_path: None,
            sandbox: SandboxBackend::UnsafeUnsandboxed,
            ownership: ExecutableOwnership::RootOnly,
            executable_identity: Some(identity),
            resource_limits: AgentResourceLimits::default(),
            enabled: true,
            timeout_ms: 2_000,
            max_stdout_bytes: 4_096,
            max_stderr_bytes: 1_024,
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        let mut service = service();
        service.agent_host = AgentHost::with_unsafe_unsandboxed_opt_in();
        service.database.upsert_agent(&agent).unwrap();

        let response = service
            .run_agent(AgentRunRequest {
                agent_id: agent.id,
                input: "test".to_owned(),
            })
            .await
            .unwrap();
        let session_id = match response {
            ProtocolResponse::AgentRun { result } => {
                assert_eq!(result.proposals.len(), 1);
                assert_eq!(
                    result.proposals[0].disposition,
                    AgentProposalDisposition::Failed
                );
                result.session.id
            }
            other => panic!("unexpected response: {other:?}"),
        };
        assert_eq!(service.database.audit_stats().unwrap().executions, 0);
        assert_eq!(
            service
                .database
                .audit_event_kinds(session_id.as_str())
                .unwrap(),
            vec![
                "agent_session_started".to_owned(),
                "agent_session_finished".to_owned(),
            ]
        );
    }

    #[tokio::test]
    async fn explicit_memory_request_commits_user_evidence() {
        let mut service = service();
        service
            .chat(
                chat("Remember that when I say \"editor\" I mean Zed."),
                "request:memory",
            )
            .await
            .unwrap();
        let memory = service
            .database
            .list_memory(Some(MemoryKind::Semantic), Some("editor"), 10)
            .unwrap();
        assert_eq!(memory.len(), 1);
        assert_eq!(memory[0].trust_classes, vec![TrustClass::UserExplicit]);
        assert!(matches!(
            &memory[0].current.value,
            MemoryValue::Preference { key, value } if key == "editor" && value == "Zed"
        ));
    }

    #[tokio::test]
    async fn ai_candidate_requires_feedback_then_same_request_is_local() {
        let fake = Arc::new(FakeProviderClient::default());
        let mut service = service();
        service.set_integrations(fake.clone(), Arc::new(FakeSecretStore::default()));
        let provider = Provider {
            id: ProviderId::generate(),
            kind: ProviderKind::OpenAiCompatible,
            name: "Local fake".to_owned(),
            base_url: "http://127.0.0.1:11434/v1".to_owned(),
            enabled: true,
            privacy: PrivacyClass::Local,
            configured: true,
            credential_id: None,
            status: ProviderStatus::Configured,
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        service.database.upsert_provider(&provider).unwrap();
        let model = AiModel {
            id: ModelId::generate(),
            provider_id: provider.id,
            display_name: "Fake model".to_owned(),
            provider_model_id: "fake-model".to_owned(),
            enabled: true,
            context_limit: Some(8_192),
            privacy: PrivacyClass::Local,
            priority: 0,
            task_eligibility: vec![AiTaskType::Conversation],
            is_default: true,
        };
        service.database.upsert_model(&model).unwrap();

        let first = service
            .chat(chat("Explain the Halquen routing idea"), "request:first")
            .await
            .unwrap();
        let candidate_id = match first {
            ProtocolResponse::Chat { result } => {
                assert_eq!(result.assistant_message.route, Some(ChatRoute::Ai));
                result.assistant_message.reusable_candidate_id.unwrap()
            }
            other => panic!("unexpected response: {other:?}"),
        };
        assert_eq!(fake.calls.load(Ordering::Relaxed), 1);
        service
            .submit_response_feedback(&candidate_id, ResponseFeedback::AlwaysUse)
            .unwrap();

        let second = service
            .chat(chat("Explain the Halquen routing idea"), "request:second")
            .await
            .unwrap();
        match second {
            ProtocolResponse::Chat { result } => {
                assert_eq!(
                    result.assistant_message.route,
                    Some(ChatRoute::ResponseCache)
                );
            }
            other => panic!("unexpected response: {other:?}"),
        }
        assert_eq!(fake.calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn cancelling_chat_drops_provider_call_without_recording_model_usage() {
        let slow = Arc::new(SlowProviderClient::default());
        let mut service = service();
        service.set_integrations(slow.clone(), Arc::new(FakeSecretStore::default()));
        let provider = Provider {
            id: ProviderId::generate(),
            kind: ProviderKind::OpenAiCompatible,
            name: "Slow local fake".to_owned(),
            base_url: "http://127.0.0.1:11434/v1".to_owned(),
            enabled: true,
            privacy: PrivacyClass::Local,
            configured: true,
            credential_id: None,
            status: ProviderStatus::Configured,
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        service.database.upsert_provider(&provider).unwrap();
        service
            .database
            .upsert_model(&AiModel {
                id: ModelId::generate(),
                provider_id: provider.id,
                display_name: "Slow fake model".to_owned(),
                provider_model_id: "slow-fake-model".to_owned(),
                enabled: true,
                context_limit: Some(8_192),
                privacy: PrivacyClass::Local,
                priority: 0,
                task_eligibility: vec![AiTaskType::Conversation],
                is_default: true,
            })
            .unwrap();

        let (sender, receiver) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            sender.send(true).unwrap();
        });
        let response = tokio::time::timeout(
            Duration::from_secs(1),
            service.chat_with_cancellation(
                chat("Use the slow provider"),
                "request:cancelled",
                receiver,
            ),
        )
        .await
        .expect("cancellation should not wait for the provider")
        .unwrap();

        match response {
            ProtocolResponse::Chat { result } => {
                assert_eq!(result.assistant_message.content, "Request cancelled.");
                assert_eq!(result.assistant_message.route, Some(ChatRoute::Unavailable));
            }
            other => panic!("unexpected response: {other:?}"),
        }
        let usage = service.database.usage_stats().unwrap();
        assert_eq!(slow.calls.load(Ordering::Relaxed), 1);
        assert_eq!(usage.model_requests, 0);
        assert_eq!(usage.failed_provider_calls, 0);
    }

    #[test]
    fn provider_secret_is_stored_outside_sqlite_and_not_returned() {
        let secrets = Arc::new(FakeSecretStore::default());
        let mut service = service();
        service.set_integrations(Arc::new(FakeProviderClient::default()), secrets.clone());
        let response = service
            .upsert_provider(ProviderUpsert {
                id: None,
                kind: ProviderKind::OpenAiCompatible,
                name: "Cloud test".to_owned(),
                base_url: "https://example.invalid/v1".to_owned(),
                enabled: true,
                privacy: PrivacyClass::Cloud,
                api_key: Some("TEST_PROVIDER_SECRET_VALUE".to_owned()),
                clear_api_key: false,
            })
            .unwrap();
        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains("TEST_PROVIDER_SECRET_VALUE"));
        assert_eq!(secrets.values.lock().unwrap().len(), 1);
    }

    #[test]
    fn invalid_provider_endpoint_does_not_mutate_secret_store() {
        let secrets = Arc::new(FakeSecretStore::default());
        let mut service = service();
        service.set_integrations(Arc::new(FakeProviderClient::default()), secrets.clone());
        let result = service.upsert_provider(ProviderUpsert {
            id: None,
            kind: ProviderKind::OpenAiCompatible,
            name: "Invalid endpoint".to_owned(),
            base_url: "http://not-loopback.invalid/v1".to_owned(),
            enabled: true,
            privacy: PrivacyClass::Cloud,
            api_key: Some("TEST_PROVIDER_SECRET_VALUE".to_owned()),
            clear_api_key: false,
        });
        assert!(result.is_err());
        assert!(secrets.values.lock().unwrap().is_empty());
    }
}
