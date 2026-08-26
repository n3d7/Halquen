use halquen_domain::{
    ActivityEvent, ActivityId, ActivityKind, AiModel, AiTaskType, AppearanceMode,
    ApplicationSettings, CacheEntryId, CachedResponse, ChatMessage, ChatMessageId, ChatOrigin,
    ChatRole, ChatRoute, ChatSession, ChatSessionId, LogLevel, ModelId, PrivacyClass, Provider,
    ProviderId, ProviderKind, ProviderStatus, ResponseFeedback, RoutingPreset, TrustClass,
    UsageStats,
};
use halquen_memory::{MemoryItem, MemoryKind, MemoryRevision, MemoryRevisionView, MemoryView};
use rusqlite::{OptionalExtension, Row, params};

use crate::{Database, StorageError};

impl Database {
    pub fn application_settings(&self) -> Result<ApplicationSettings, StorageError> {
        self.connection
            .query_row(
                "SELECT appearance, language, allow_cloud_ai, allow_local_ai,
                        allow_personal_context, routing_preset, max_model_calls_per_request,
                        max_context_tokens, max_output_tokens, prefer_cached_local,
                        allow_expensive_fallback, personal_instructions, learning_enabled,
                        ask_before_procedural_rules, auto_save_explicit_preferences,
                        conversation_retention_days, episodic_retention_days, log_level,
                        diagnostic_logging, log_retention_days, log_max_total_mb
                 FROM application_settings WHERE singleton_id = 1",
                [],
                |row| {
                    Ok(ApplicationSettings {
                        appearance: parse_appearance(&row.get::<_, String>(0)?)?,
                        language: row.get(1)?,
                        allow_cloud_ai: row.get(2)?,
                        allow_local_ai: row.get(3)?,
                        allow_personal_context: row.get(4)?,
                        routing_preset: parse_routing_preset(&row.get::<_, String>(5)?)?,
                        max_model_calls_per_request: row.get(6)?,
                        max_context_tokens: row.get(7)?,
                        max_output_tokens: row.get(8)?,
                        prefer_cached_local: row.get(9)?,
                        allow_expensive_fallback: row.get(10)?,
                        personal_instructions: row.get(11)?,
                        learning_enabled: row.get(12)?,
                        ask_before_procedural_rules: row.get(13)?,
                        auto_save_explicit_preferences: row.get(14)?,
                        conversation_retention_days: row.get(15)?,
                        episodic_retention_days: row.get(16)?,
                        log_level: parse_log_level(&row.get::<_, String>(17)?)?,
                        diagnostic_logging: row.get(18)?,
                        log_retention_days: row.get(19)?,
                        log_max_total_mb: row.get(20)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn update_application_settings(
        &mut self,
        settings: &ApplicationSettings,
        now_ms: i64,
    ) -> Result<(), StorageError> {
        settings
            .validate()
            .map_err(|error| StorageError::InvalidInteraction(error.to_string()))?;
        let updated = self.connection.execute(
            "UPDATE application_settings SET
                appearance = ?1, language = ?2, allow_cloud_ai = ?3, allow_local_ai = ?4,
                allow_personal_context = ?5, routing_preset = ?6,
                max_model_calls_per_request = ?7, max_context_tokens = ?8,
                max_output_tokens = ?9, prefer_cached_local = ?10,
                allow_expensive_fallback = ?11, personal_instructions = ?12,
                learning_enabled = ?13, ask_before_procedural_rules = ?14,
                auto_save_explicit_preferences = ?15, conversation_retention_days = ?16,
                episodic_retention_days = ?17, log_level = ?18, diagnostic_logging = ?19,
                log_retention_days = ?20, log_max_total_mb = ?21, updated_at_ms = ?22
             WHERE singleton_id = 1",
            params![
                appearance(settings.appearance),
                settings.language,
                settings.allow_cloud_ai,
                settings.allow_local_ai,
                settings.allow_personal_context,
                routing_preset(settings.routing_preset),
                settings.max_model_calls_per_request,
                settings.max_context_tokens,
                settings.max_output_tokens,
                settings.prefer_cached_local,
                settings.allow_expensive_fallback,
                settings.personal_instructions,
                settings.learning_enabled,
                settings.ask_before_procedural_rules,
                settings.auto_save_explicit_preferences,
                settings.conversation_retention_days,
                settings.episodic_retention_days,
                log_level(settings.log_level),
                settings.diagnostic_logging,
                settings.log_retention_days,
                settings.log_max_total_mb,
                now_ms,
            ],
        )?;
        if updated != 1 {
            return Err(StorageError::InvalidInteraction(
                "settings singleton is missing".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn list_providers(&self) -> Result<Vec<Provider>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, provider_kind, name, base_url, enabled, privacy_class,
                    credential_id, status, created_at_ms, updated_at_ms
             FROM ai_providers ORDER BY name COLLATE NOCASE, id",
        )?;
        statement
            .query_map([], provider_from_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn provider(&self, id: &ProviderId) -> Result<Option<Provider>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, provider_kind, name, base_url, enabled, privacy_class,
                        credential_id, status, created_at_ms, updated_at_ms
                 FROM ai_providers WHERE id = ?1",
                [id.as_str()],
                provider_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn upsert_provider(&mut self, provider: &Provider) -> Result<(), StorageError> {
        if provider.name.trim().is_empty()
            || provider.name.len() > 128
            || provider.base_url.trim().is_empty()
            || provider.base_url.len() > 2_048
        {
            return Err(StorageError::InvalidInteraction(
                "provider metadata is outside accepted bounds".to_owned(),
            ));
        }
        self.connection.execute(
            "INSERT INTO ai_providers(
                id, provider_kind, name, base_url, enabled, privacy_class, credential_id,
                status, created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                provider_kind = excluded.provider_kind,
                name = excluded.name,
                base_url = excluded.base_url,
                enabled = excluded.enabled,
                privacy_class = excluded.privacy_class,
                credential_id = excluded.credential_id,
                status = excluded.status,
                updated_at_ms = excluded.updated_at_ms",
            params![
                provider.id.as_str(),
                provider_kind(provider.kind),
                provider.name,
                provider.base_url,
                provider.enabled,
                privacy_class(provider.privacy),
                provider.credential_id,
                provider_status(provider.status),
                provider.created_at_ms,
                provider.updated_at_ms,
            ],
        )?;
        Ok(())
    }

    pub fn remove_provider(&mut self, id: &ProviderId) -> Result<bool, StorageError> {
        Ok(self
            .connection
            .execute("DELETE FROM ai_providers WHERE id = ?1", [id.as_str()])?
            == 1)
    }

    pub fn list_models(&self) -> Result<Vec<AiModel>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, provider_id, display_name, provider_model_id, enabled, context_limit,
                    privacy_class, priority, is_default
             FROM ai_models ORDER BY is_default DESC, priority DESC, display_name COLLATE NOCASE",
        )?;
        let base = statement
            .query_map([], model_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        base.into_iter()
            .map(|mut model| {
                model.task_eligibility = self.model_tasks(&model.id)?;
                Ok(model)
            })
            .collect()
    }

    pub fn model(&self, id: &ModelId) -> Result<Option<AiModel>, StorageError> {
        let model = self
            .connection
            .query_row(
                "SELECT id, provider_id, display_name, provider_model_id, enabled, context_limit,
                        privacy_class, priority, is_default
                 FROM ai_models WHERE id = ?1",
                [id.as_str()],
                model_from_row,
            )
            .optional()?;
        model
            .map(|mut model| {
                model.task_eligibility = self.model_tasks(&model.id)?;
                Ok(model)
            })
            .transpose()
    }

    fn model_tasks(&self, id: &ModelId) -> Result<Vec<AiTaskType>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT task_type FROM ai_model_tasks WHERE model_id = ?1 ORDER BY task_type",
        )?;
        let values = statement
            .query_map([id.as_str()], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        values
            .into_iter()
            .map(|value| parse_ai_task(&value).map_err(StorageError::Database))
            .collect()
    }

    pub fn upsert_model(&mut self, model: &AiModel) -> Result<(), StorageError> {
        if model.display_name.trim().is_empty()
            || model.display_name.len() > 128
            || model.provider_model_id.trim().is_empty()
            || model.provider_model_id.len() > 256
            || model.task_eligibility.is_empty()
            || !(-1_000..=1_000).contains(&model.priority)
        {
            return Err(StorageError::InvalidInteraction(
                "model metadata is outside accepted bounds".to_owned(),
            ));
        }
        let transaction = self.connection.transaction()?;
        if model.is_default {
            transaction.execute(
                "UPDATE ai_models SET is_default = 0 WHERE provider_id = ?1 AND id <> ?2",
                params![model.provider_id.as_str(), model.id.as_str()],
            )?;
        }
        transaction.execute(
            "INSERT INTO ai_models(
                id, provider_id, display_name, provider_model_id, enabled, context_limit,
                privacy_class, priority, is_default
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                provider_id = excluded.provider_id,
                display_name = excluded.display_name,
                provider_model_id = excluded.provider_model_id,
                enabled = excluded.enabled,
                context_limit = excluded.context_limit,
                privacy_class = excluded.privacy_class,
                priority = excluded.priority,
                is_default = excluded.is_default",
            params![
                model.id.as_str(),
                model.provider_id.as_str(),
                model.display_name,
                model.provider_model_id,
                model.enabled,
                model.context_limit,
                privacy_class(model.privacy),
                model.priority,
                model.is_default,
            ],
        )?;
        transaction.execute(
            "DELETE FROM ai_model_tasks WHERE model_id = ?1",
            [model.id.as_str()],
        )?;
        for task in &model.task_eligibility {
            transaction.execute(
                "INSERT INTO ai_model_tasks(model_id, task_type) VALUES (?1, ?2)",
                params![model.id.as_str(), ai_task(*task)],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn ensure_chat_session(
        &mut self,
        requested: Option<ChatSessionId>,
        title: &str,
        now_ms: i64,
    ) -> Result<ChatSession, StorageError> {
        if let Some(id) = requested {
            if let Some(session) = self.chat_session(&id)? {
                return Ok(session);
            }
            return Err(StorageError::InvalidInteraction(
                "requested chat session does not exist".to_owned(),
            ));
        }
        let title = bounded_title(title);
        let session = ChatSession {
            id: ChatSessionId::generate(),
            title,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
        };
        self.connection.execute(
            "INSERT INTO chat_sessions(id, title, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                session.id.as_str(),
                session.title,
                session.created_at_ms,
                session.updated_at_ms
            ],
        )?;
        Ok(session)
    }

    pub fn chat_session(&self, id: &ChatSessionId) -> Result<Option<ChatSession>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, title, created_at_ms, updated_at_ms FROM chat_sessions WHERE id = ?1",
                [id.as_str()],
                chat_session_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_chat_sessions(&self, limit: u16) -> Result<Vec<ChatSession>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, title, created_at_ms, updated_at_ms
             FROM chat_sessions ORDER BY updated_at_ms DESC LIMIT ?1",
        )?;
        statement
            .query_map([i64::from(limit.clamp(1, 200))], chat_session_from_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn append_chat_message(&mut self, message: &ChatMessage) -> Result<(), StorageError> {
        if message.content.trim().is_empty() || message.content.len() > 65_536 {
            return Err(StorageError::InvalidInteraction(
                "chat message is empty or too large".to_owned(),
            ));
        }
        let latency_ms = message.latency_ms.map(sql_i64).transpose()?;
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO chat_messages(
                id, session_id, role, content, origin, route, provider_id, model_id,
                input_tokens, output_tokens, latency_ms, reusable_candidate_id, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                message.id.as_str(),
                message.session_id.as_str(),
                chat_role(message.role),
                message.content,
                chat_origin(message.origin),
                message.route.map(chat_route),
                message.provider_id.as_ref().map(ProviderId::as_str),
                message.model_id.as_ref().map(ModelId::as_str),
                message.input_tokens,
                message.output_tokens,
                latency_ms,
                message
                    .reusable_candidate_id
                    .as_ref()
                    .map(CacheEntryId::as_str),
                message.created_at_ms,
            ],
        )?;
        transaction.execute(
            "UPDATE chat_sessions SET updated_at_ms = ?1 WHERE id = ?2",
            params![message.created_at_ms, message.session_id.as_str()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn list_chat_messages(
        &self,
        session_id: &ChatSessionId,
        limit: u16,
    ) -> Result<Vec<ChatMessage>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, session_id, role, content, origin, route, provider_id, model_id,
                    input_tokens, output_tokens, latency_ms, reusable_candidate_id, created_at_ms
             FROM chat_messages WHERE session_id = ?1
             ORDER BY created_at_ms DESC, rowid DESC LIMIT ?2",
        )?;
        let mut messages = statement
            .query_map(
                params![session_id.as_str(), i64::from(limit.clamp(1, 500))],
                chat_message_from_row,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        messages.reverse();
        Ok(messages)
    }

    pub fn append_activity(&mut self, event: &ActivityEvent) -> Result<(), StorageError> {
        if event.correlation_id.is_empty()
            || event.correlation_id.len() > 128
            || event.summary.trim().is_empty()
            || event.summary.len() > 1_024
            || event
                .detail
                .as_ref()
                .is_some_and(|value| value.len() > 4_096)
        {
            return Err(StorageError::InvalidInteraction(
                "activity event is outside accepted bounds".to_owned(),
            ));
        }
        self.connection.execute(
            "INSERT INTO activity_events(
                id, session_id, correlation_id, activity_kind, summary, detail, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                event.id.as_str(),
                event.session_id.as_ref().map(ChatSessionId::as_str),
                event.correlation_id,
                activity_kind(event.kind),
                event.summary,
                event.detail,
                event.created_at_ms,
            ],
        )?;
        Ok(())
    }

    pub fn list_activity(&self, limit: u16) -> Result<Vec<ActivityEvent>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, session_id, correlation_id, activity_kind, summary, detail, created_at_ms
             FROM activity_events ORDER BY created_at_ms DESC, rowid DESC LIMIT ?1",
        )?;
        statement
            .query_map([i64::from(limit.clamp(1, 500))], activity_from_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn cached_response(
        &self,
        normalized_request: &str,
        context_key: &str,
        now_ms: i64,
    ) -> Result<Option<CachedResponse>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, normalized_request, response, context_key, confidence_permille,
                        priority_permille, trust_class, valid_until_ms, reusable, created_at_ms,
                        last_used_at_ms, usage_count, success_count, correction_count,
                        original_provider_id, original_model_id, estimated_tokens_avoided
                 FROM response_cache
                 WHERE normalized_request = ?1 AND context_key = ?2 AND reusable = 1
                   AND confidence_permille >= 800
                   AND (valid_until_ms IS NULL OR valid_until_ms >= ?3)",
                params![normalized_request, context_key, now_ms],
                cached_response_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn store_response_candidate(&mut self, entry: &CachedResponse) -> Result<(), StorageError> {
        if entry.normalized_request.is_empty()
            || entry.normalized_request.len() > 4_096
            || entry.response.is_empty()
            || entry.response.len() > 65_536
            || entry.context_key.is_empty()
            || entry.context_key.len() > 512
            || entry.confidence_permille > 1_000
            || entry.priority_permille > 1_000
        {
            return Err(StorageError::InvalidInteraction(
                "response cache candidate is outside accepted bounds".to_owned(),
            ));
        }
        let usage_count = sql_i64(entry.usage_count)?;
        let success_count = sql_i64(entry.success_count)?;
        let correction_count = sql_i64(entry.correction_count)?;
        let estimated_tokens_avoided = sql_i64(entry.estimated_tokens_avoided)?;
        self.connection.execute(
            "INSERT INTO response_cache(
                id, normalized_request, response, context_key, confidence_permille,
                priority_permille, trust_class, valid_until_ms, reusable, created_at_ms,
                last_used_at_ms, usage_count, success_count, correction_count,
                original_provider_id, original_model_id, estimated_tokens_avoided
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
             ON CONFLICT(normalized_request, context_key) DO UPDATE SET
                id = excluded.id,
                response = excluded.response,
                confidence_permille = excluded.confidence_permille,
                priority_permille = excluded.priority_permille,
                trust_class = excluded.trust_class,
                valid_until_ms = excluded.valid_until_ms,
                reusable = excluded.reusable,
                created_at_ms = excluded.created_at_ms,
                last_used_at_ms = excluded.last_used_at_ms,
                usage_count = excluded.usage_count,
                success_count = excluded.success_count,
                correction_count = excluded.correction_count,
                original_provider_id = excluded.original_provider_id,
                original_model_id = excluded.original_model_id,
                estimated_tokens_avoided = excluded.estimated_tokens_avoided",
            params![
                entry.id.as_str(),
                entry.normalized_request,
                entry.response,
                entry.context_key,
                entry.confidence_permille,
                entry.priority_permille,
                trust_class(entry.trust),
                entry.valid_until_ms,
                entry.reusable,
                entry.created_at_ms,
                entry.last_used_at_ms,
                usage_count,
                success_count,
                correction_count,
                entry.original_provider_id.as_ref().map(ProviderId::as_str),
                entry.original_model_id.as_ref().map(ModelId::as_str),
                estimated_tokens_avoided,
            ],
        )?;
        Ok(())
    }

    pub fn record_cache_hit(
        &mut self,
        id: &CacheEntryId,
        now_ms: i64,
        estimated_tokens_avoided: u64,
    ) -> Result<(), StorageError> {
        let estimated_tokens_avoided = sql_i64(estimated_tokens_avoided)?;
        let transaction = self.connection.transaction()?;
        let updated = transaction.execute(
            "UPDATE response_cache SET
                last_used_at_ms = ?1,
                usage_count = usage_count + 1,
                estimated_tokens_avoided = estimated_tokens_avoided + ?2
             WHERE id = ?3",
            params![now_ms, estimated_tokens_avoided, id.as_str()],
        )?;
        if updated != 1 {
            return Err(StorageError::InvalidInteraction(
                "cache entry was not found".to_owned(),
            ));
        }
        transaction.execute(
            "UPDATE usage_stats SET
                response_cache_hits = response_cache_hits + 1,
                estimated_tokens_avoided = estimated_tokens_avoided + ?1
             WHERE singleton_id = 1",
            [estimated_tokens_avoided],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn apply_response_feedback(
        &mut self,
        id: &CacheEntryId,
        feedback: ResponseFeedback,
    ) -> Result<(), StorageError> {
        let (confidence_delta, priority_delta, success_delta, correction_delta, reusable) =
            match feedback {
                ResponseFeedback::Useful => (50, 20, 1, 0, None),
                ResponseFeedback::Wrong => (-500, -200, 0, 1, Some(false)),
                ResponseFeedback::DoNotRemember => (0, 0, 0, 0, Some(false)),
                ResponseFeedback::AlwaysUse => (500, 300, 1, 0, Some(true)),
                ResponseFeedback::Prefer => (100, 150, 1, 0, None),
            };
        let updated = self.connection.execute(
            "UPDATE response_cache SET
                confidence_permille = min(1000, max(0, confidence_permille + ?1)),
                priority_permille = min(1000, max(0, priority_permille + ?2)),
                success_count = success_count + ?3,
                correction_count = correction_count + ?4,
                reusable = COALESCE(?5, reusable)
             WHERE id = ?6",
            params![
                confidence_delta,
                priority_delta,
                success_delta,
                correction_delta,
                reusable,
                id.as_str()
            ],
        )?;
        if updated != 1 {
            return Err(StorageError::InvalidInteraction(
                "cache entry was not found".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn usage_stats(&self) -> Result<UsageStats, StorageError> {
        self.connection
            .query_row(
                "SELECT model_requests, input_tokens, output_tokens, cached_tokens,
                        ai_fallbacks, local_resolutions, response_cache_hits, clarifications,
                        failed_provider_calls, estimated_tokens_avoided
                 FROM usage_stats WHERE singleton_id = 1",
                [],
                |row| {
                    Ok(UsageStats {
                        model_requests: row_u64(row, 0)?,
                        input_tokens: row_u64(row, 1)?,
                        output_tokens: row_u64(row, 2)?,
                        cached_tokens: row_u64(row, 3)?,
                        ai_fallbacks: row_u64(row, 4)?,
                        local_resolutions: row_u64(row, 5)?,
                        response_cache_hits: row_u64(row, 6)?,
                        clarifications: row_u64(row, 7)?,
                        failed_provider_calls: row_u64(row, 8)?,
                        estimated_tokens_avoided: row_u64(row, 9)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn cached_response_count(&self) -> Result<u64, StorageError> {
        let count: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM response_cache", [], |row| row.get(0))?;
        u64::try_from(count)
            .map_err(|_| StorageError::InvalidInteraction("negative cache count".to_owned()))
    }

    pub fn add_usage(&mut self, delta: UsageStats) -> Result<(), StorageError> {
        let values = [
            sql_i64(delta.model_requests)?,
            sql_i64(delta.input_tokens)?,
            sql_i64(delta.output_tokens)?,
            sql_i64(delta.cached_tokens)?,
            sql_i64(delta.ai_fallbacks)?,
            sql_i64(delta.local_resolutions)?,
            sql_i64(delta.response_cache_hits)?,
            sql_i64(delta.clarifications)?,
            sql_i64(delta.failed_provider_calls)?,
            sql_i64(delta.estimated_tokens_avoided)?,
        ];
        self.connection.execute(
            "UPDATE usage_stats SET
                model_requests = model_requests + ?1,
                input_tokens = input_tokens + ?2,
                output_tokens = output_tokens + ?3,
                cached_tokens = cached_tokens + ?4,
                ai_fallbacks = ai_fallbacks + ?5,
                local_resolutions = local_resolutions + ?6,
                response_cache_hits = response_cache_hits + ?7,
                clarifications = clarifications + ?8,
                failed_provider_calls = failed_provider_calls + ?9,
                estimated_tokens_avoided = estimated_tokens_avoided + ?10
             WHERE singleton_id = 1",
            params![
                values[0], values[1], values[2], values[3], values[4], values[5], values[6],
                values[7], values[8], values[9],
            ],
        )?;
        Ok(())
    }

    pub fn list_memory(
        &self,
        kind: Option<MemoryKind>,
        search: Option<&str>,
        limit: u16,
    ) -> Result<Vec<MemoryView>, StorageError> {
        let kind = kind.map(memory_kind);
        let search = search.map(|value| format!("%{}%", value.replace('%', "\\%")));
        let mut statement = self.connection.prepare(
            "SELECT mi.id, mi.memory_kind, mi.current_revision_id, mi.created_at_ms,
                    mi.updated_at_ms, mi.priority_permille, mi.confidence_permille,
                    mi.pinned, mi.disabled, mi.last_used_at_ms,
                    mr.previous_revision_id, mr.value_json, mr.created_at_ms,
                    mr.valid_from_ms, mr.valid_until_ms,
                    (SELECT COUNT(*) FROM memory_evidence me WHERE me.revision_id = mr.id)
             FROM memory_items mi
             JOIN memory_revisions mr ON mr.id = mi.current_revision_id
             WHERE (?1 IS NULL OR mi.memory_kind = ?1)
               AND (?2 IS NULL OR mr.value_json LIKE ?2 ESCAPE '\\' OR mi.id LIKE ?2 ESCAPE '\\')
             ORDER BY mi.pinned DESC, mi.priority_permille DESC, mi.updated_at_ms DESC
             LIMIT ?3",
        )?;
        let rows = statement.query_map(
            params![kind, search, i64::from(limit.clamp(1, 500))],
            |row| {
                let item = memory_item_from_row(row)?;
                let revision = MemoryRevision {
                    id: item.current_revision_id.clone(),
                    memory_id: item.id.clone(),
                    previous_revision_id: optional_memory_revision_id(row.get(10)?)?,
                    value: serde_json::from_str(&row.get::<_, String>(11)?).map_err(json_error)?,
                    evidence_ids: Vec::new(),
                    created_at_ms: row.get(12)?,
                    valid_from_ms: row.get(13)?,
                    valid_until_ms: row.get(14)?,
                };
                Ok((
                    item,
                    revision,
                    row_u64(row, 15)?,
                    row.get::<_, u16>(5)?,
                    row.get::<_, u16>(6)?,
                    row.get::<_, bool>(7)?,
                    row.get::<_, bool>(8)?,
                    row.get::<_, Option<i64>>(9)?,
                ))
            },
        )?;
        let base = rows.collect::<Result<Vec<_>, _>>()?;
        base.into_iter()
            .map(
                |(
                    item,
                    mut current,
                    evidence_count,
                    priority_permille,
                    confidence_permille,
                    pinned,
                    disabled,
                    last_used_at_ms,
                )| {
                    let (evidence_ids, trust_classes) = self.revision_evidence(&current.id)?;
                    current.evidence_ids = evidence_ids;
                    Ok(MemoryView {
                        item,
                        current,
                        evidence_count,
                        trust_classes,
                        priority_permille,
                        confidence_permille,
                        pinned,
                        disabled,
                        last_used_at_ms,
                    })
                },
            )
            .collect()
    }

    pub fn memory_history(
        &self,
        memory_id: &halquen_domain::MemoryId,
    ) -> Result<Vec<MemoryRevisionView>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, previous_revision_id, value_json, created_at_ms, valid_from_ms, valid_until_ms
             FROM memory_revisions WHERE memory_id = ?1 ORDER BY created_at_ms DESC, rowid DESC",
        )?;
        let revisions = statement
            .query_map([memory_id.as_str()], |row| {
                Ok(MemoryRevision {
                    id: memory_revision_id(row.get(0)?)?,
                    memory_id: memory_id.clone(),
                    previous_revision_id: optional_memory_revision_id(row.get(1)?)?,
                    value: serde_json::from_str(&row.get::<_, String>(2)?).map_err(json_error)?,
                    evidence_ids: Vec::new(),
                    created_at_ms: row.get(3)?,
                    valid_from_ms: row.get(4)?,
                    valid_until_ms: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        revisions
            .into_iter()
            .map(|mut revision| {
                let (evidence_ids, trust_classes) = self.revision_evidence(&revision.id)?;
                revision.evidence_ids = evidence_ids;
                Ok(MemoryRevisionView {
                    revision,
                    trust_classes,
                })
            })
            .collect()
    }

    pub fn memory_head(
        &self,
        memory_id: &halquen_domain::MemoryId,
    ) -> Result<Option<(MemoryItem, MemoryRevision)>, StorageError> {
        let row = self
            .connection
            .query_row(
                "SELECT mi.id, mi.memory_kind, mi.current_revision_id, mi.created_at_ms,
                        mi.updated_at_ms, mr.previous_revision_id, mr.value_json,
                        mr.created_at_ms, mr.valid_from_ms, mr.valid_until_ms
                 FROM memory_items mi
                 JOIN memory_revisions mr ON mr.id = mi.current_revision_id
                 WHERE mi.id = ?1",
                [memory_id.as_str()],
                |row| {
                    let item = MemoryItem {
                        id: halquen_domain::MemoryId::new(row.get::<_, String>(0)?)
                            .map_err(|_| sqlite_enum_error())?,
                        kind: parse_memory_kind(&row.get::<_, String>(1)?)?,
                        current_revision_id: memory_revision_id(row.get(2)?)?,
                        created_at_ms: row.get(3)?,
                        updated_at_ms: row.get(4)?,
                    };
                    let revision = MemoryRevision {
                        id: item.current_revision_id.clone(),
                        memory_id: item.id.clone(),
                        previous_revision_id: optional_memory_revision_id(row.get(5)?)?,
                        value: serde_json::from_str(&row.get::<_, String>(6)?)
                            .map_err(json_error)?,
                        evidence_ids: Vec::new(),
                        created_at_ms: row.get(7)?,
                        valid_from_ms: row.get(8)?,
                        valid_until_ms: row.get(9)?,
                    };
                    Ok((item, revision))
                },
            )
            .optional()?;
        row.map(|(item, mut revision)| {
            revision.evidence_ids = self.revision_evidence(&revision.id)?.0;
            Ok((item, revision))
        })
        .transpose()
    }

    pub fn memory_revision(
        &self,
        memory_id: &halquen_domain::MemoryId,
        revision_id: &halquen_domain::MemoryRevisionId,
    ) -> Result<Option<MemoryRevision>, StorageError> {
        let revision = self
            .connection
            .query_row(
                "SELECT previous_revision_id, value_json, created_at_ms, valid_from_ms, valid_until_ms
                 FROM memory_revisions WHERE id = ?1 AND memory_id = ?2",
                params![revision_id.as_str(), memory_id.as_str()],
                |row| {
                    Ok(MemoryRevision {
                        id: revision_id.clone(),
                        memory_id: memory_id.clone(),
                        previous_revision_id: optional_memory_revision_id(row.get(0)?)?,
                        value: serde_json::from_str(&row.get::<_, String>(1)?).map_err(json_error)?,
                        evidence_ids: Vec::new(),
                        created_at_ms: row.get(2)?,
                        valid_from_ms: row.get(3)?,
                        valid_until_ms: row.get(4)?,
                    })
                },
            )
            .optional()?;
        revision
            .map(|mut revision| {
                revision.evidence_ids = self.revision_evidence(&revision.id)?.0;
                Ok(revision)
            })
            .transpose()
    }

    pub fn preference_by_key(
        &self,
        key: &str,
    ) -> Result<Option<(MemoryItem, MemoryRevision)>, StorageError> {
        let row = self
            .connection
            .query_row(
                "SELECT mi.id, mi.memory_kind, mi.current_revision_id, mi.created_at_ms,
                        mi.updated_at_ms, mr.previous_revision_id, mr.value_json,
                        mr.created_at_ms, mr.valid_from_ms, mr.valid_until_ms
                 FROM memory_items mi
                 JOIN memory_revisions mr ON mr.id = mi.current_revision_id
                 WHERE mi.disabled = 0
                   AND json_extract(mr.value_json, '$.kind') = 'preference'
                   AND json_extract(mr.value_json, '$.key') = ?1
                 LIMIT 1",
                [key],
                |row| {
                    let item = MemoryItem {
                        id: memory_id(row.get(0)?)?,
                        kind: parse_memory_kind(&row.get::<_, String>(1)?)?,
                        current_revision_id: memory_revision_id(row.get(2)?)?,
                        created_at_ms: row.get(3)?,
                        updated_at_ms: row.get(4)?,
                    };
                    let revision = MemoryRevision {
                        id: item.current_revision_id.clone(),
                        memory_id: item.id.clone(),
                        previous_revision_id: optional_memory_revision_id(row.get(5)?)?,
                        value: serde_json::from_str(&row.get::<_, String>(6)?)
                            .map_err(json_error)?,
                        evidence_ids: Vec::new(),
                        created_at_ms: row.get(7)?,
                        valid_from_ms: row.get(8)?,
                        valid_until_ms: row.get(9)?,
                    };
                    Ok((item, revision))
                },
            )
            .optional()?;
        row.map(|(item, mut revision)| {
            revision.evidence_ids = self.revision_evidence(&revision.id)?.0;
            Ok((item, revision))
        })
        .transpose()
    }

    pub fn set_memory_state(
        &mut self,
        memory_id: &halquen_domain::MemoryId,
        pinned: Option<bool>,
        disabled: Option<bool>,
        priority_permille: Option<u16>,
    ) -> Result<bool, StorageError> {
        if priority_permille.is_some_and(|value| value > 1_000) {
            return Err(StorageError::InvalidInteraction(
                "memory priority exceeds 1000".to_owned(),
            ));
        }
        Ok(self.connection.execute(
            "UPDATE memory_items SET
                pinned = COALESCE(?1, pinned),
                disabled = COALESCE(?2, disabled),
                priority_permille = COALESCE(?3, priority_permille)
             WHERE id = ?4",
            params![pinned, disabled, priority_permille, memory_id.as_str()],
        )? == 1)
    }

    fn revision_evidence(
        &self,
        revision_id: &halquen_domain::MemoryRevisionId,
    ) -> Result<(Vec<halquen_domain::EvidenceId>, Vec<TrustClass>), StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT e.id, e.trust_class FROM memory_evidence me
             JOIN evidence e ON e.id = me.evidence_id
             WHERE me.revision_id = ?1 ORDER BY e.created_at_ms, e.id",
        )?;
        let rows = statement
            .query_map([revision_id.as_str()], |row| {
                Ok((
                    evidence_id(row.get(0)?)?,
                    parse_trust_class(&row.get::<_, String>(1)?)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows.into_iter().unzip())
    }
}

fn provider_from_row(row: &Row<'_>) -> rusqlite::Result<Provider> {
    let privacy = parse_privacy_class(&row.get::<_, String>(5)?)?;
    let credential_id: Option<String> = row.get(6)?;
    Ok(Provider {
        id: provider_id(row.get(0)?)?,
        kind: parse_provider_kind(&row.get::<_, String>(1)?)?,
        name: row.get(2)?,
        base_url: row.get(3)?,
        enabled: row.get(4)?,
        privacy,
        configured: credential_id.is_some() || privacy == PrivacyClass::Local,
        credential_id,
        status: parse_provider_status(&row.get::<_, String>(7)?)?,
        created_at_ms: row.get(8)?,
        updated_at_ms: row.get(9)?,
    })
}

fn model_from_row(row: &Row<'_>) -> rusqlite::Result<AiModel> {
    Ok(AiModel {
        id: model_id(row.get(0)?)?,
        provider_id: provider_id(row.get(1)?)?,
        display_name: row.get(2)?,
        provider_model_id: row.get(3)?,
        enabled: row.get(4)?,
        context_limit: row.get(5)?,
        privacy: parse_privacy_class(&row.get::<_, String>(6)?)?,
        priority: row.get(7)?,
        task_eligibility: Vec::new(),
        is_default: row.get(8)?,
    })
}

fn chat_session_from_row(row: &Row<'_>) -> rusqlite::Result<ChatSession> {
    Ok(ChatSession {
        id: chat_session_id(row.get(0)?)?,
        title: row.get(1)?,
        created_at_ms: row.get(2)?,
        updated_at_ms: row.get(3)?,
    })
}

fn chat_message_from_row(row: &Row<'_>) -> rusqlite::Result<ChatMessage> {
    Ok(ChatMessage {
        id: chat_message_id(row.get(0)?)?,
        session_id: chat_session_id(row.get(1)?)?,
        role: parse_chat_role(&row.get::<_, String>(2)?)?,
        content: row.get(3)?,
        origin: parse_chat_origin(&row.get::<_, String>(4)?)?,
        route: row
            .get::<_, Option<String>>(5)?
            .map(|value| parse_chat_route(&value))
            .transpose()?,
        provider_id: row
            .get::<_, Option<String>>(6)?
            .map(provider_id)
            .transpose()?,
        model_id: row.get::<_, Option<String>>(7)?.map(model_id).transpose()?,
        input_tokens: row.get(8)?,
        output_tokens: row.get(9)?,
        latency_ms: row
            .get::<_, Option<i64>>(10)?
            .map(|value| u64::try_from(value).map_err(|_| sqlite_enum_error()))
            .transpose()?,
        reusable_candidate_id: row
            .get::<_, Option<String>>(11)?
            .map(cache_entry_id)
            .transpose()?,
        created_at_ms: row.get(12)?,
    })
}

fn activity_from_row(row: &Row<'_>) -> rusqlite::Result<ActivityEvent> {
    Ok(ActivityEvent {
        id: activity_id(row.get(0)?)?,
        session_id: row
            .get::<_, Option<String>>(1)?
            .map(chat_session_id)
            .transpose()?,
        correlation_id: row.get(2)?,
        kind: parse_activity_kind(&row.get::<_, String>(3)?)?,
        summary: row.get(4)?,
        detail: row.get(5)?,
        created_at_ms: row.get(6)?,
    })
}

fn cached_response_from_row(row: &Row<'_>) -> rusqlite::Result<CachedResponse> {
    Ok(CachedResponse {
        id: cache_entry_id(row.get(0)?)?,
        normalized_request: row.get(1)?,
        response: row.get(2)?,
        context_key: row.get(3)?,
        confidence_permille: row.get(4)?,
        priority_permille: row.get(5)?,
        trust: parse_trust_class(&row.get::<_, String>(6)?)?,
        valid_until_ms: row.get(7)?,
        reusable: row.get(8)?,
        created_at_ms: row.get(9)?,
        last_used_at_ms: row.get(10)?,
        usage_count: row_u64(row, 11)?,
        success_count: row_u64(row, 12)?,
        correction_count: row_u64(row, 13)?,
        original_provider_id: row
            .get::<_, Option<String>>(14)?
            .map(provider_id)
            .transpose()?,
        original_model_id: row
            .get::<_, Option<String>>(15)?
            .map(model_id)
            .transpose()?,
        estimated_tokens_avoided: row_u64(row, 16)?,
    })
}

fn memory_item_from_row(row: &Row<'_>) -> rusqlite::Result<MemoryItem> {
    Ok(MemoryItem {
        id: memory_id(row.get(0)?)?,
        kind: parse_memory_kind(&row.get::<_, String>(1)?)?,
        current_revision_id: memory_revision_id(row.get(2)?)?,
        created_at_ms: row.get(3)?,
        updated_at_ms: row.get(4)?,
    })
}

fn bounded_title(value: &str) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let value = if normalized.is_empty() {
        "New conversation"
    } else {
        &normalized
    };
    value.chars().take(80).collect()
}

fn sqlite_enum_error() -> rusqlite::Error {
    rusqlite::Error::InvalidQuery
}

fn sql_i64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| {
        StorageError::InvalidInteraction("numeric value exceeds SQLite integer range".to_owned())
    })
}

fn row_u64(row: &Row<'_>, index: usize) -> rusqlite::Result<u64> {
    u64::try_from(row.get::<_, i64>(index)?).map_err(|_| sqlite_enum_error())
}

fn json_error(_: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::InvalidQuery
}

macro_rules! parse_id {
    ($name:ident, $type:ty) => {
        fn $name(value: String) -> rusqlite::Result<$type> {
            <$type>::new(value).map_err(|_| sqlite_enum_error())
        }
    };
}

parse_id!(activity_id, ActivityId);
parse_id!(cache_entry_id, CacheEntryId);
parse_id!(chat_message_id, ChatMessageId);
parse_id!(chat_session_id, ChatSessionId);
parse_id!(evidence_id, halquen_domain::EvidenceId);
parse_id!(memory_id, halquen_domain::MemoryId);
parse_id!(memory_revision_id, halquen_domain::MemoryRevisionId);
parse_id!(model_id, ModelId);
parse_id!(provider_id, ProviderId);

fn optional_memory_revision_id(
    value: Option<String>,
) -> rusqlite::Result<Option<halquen_domain::MemoryRevisionId>> {
    value.map(memory_revision_id).transpose()
}

macro_rules! enum_mapping {
    ($serialize:ident, $parse:ident, $type:ty, {$($variant:path => $value:literal),+ $(,)?}) => {
        fn $serialize(value: $type) -> &'static str {
            match value { $($variant => $value),+ }
        }
        fn $parse(value: &str) -> rusqlite::Result<$type> {
            match value { $($value => Ok($variant)),+, _ => Err(sqlite_enum_error()) }
        }
    };
}

enum_mapping!(appearance, parse_appearance, AppearanceMode, {
    AppearanceMode::System => "system", AppearanceMode::Light => "light", AppearanceMode::Dark => "dark"
});
enum_mapping!(routing_preset, parse_routing_preset, RoutingPreset, {
    RoutingPreset::Balanced => "balanced", RoutingPreset::MinimizeAiUsage => "minimize_ai_usage",
    RoutingPreset::MinimizeCost => "minimize_cost", RoutingPreset::PreferLocal => "prefer_local",
    RoutingPreset::PreferQuality => "prefer_quality", RoutingPreset::Custom => "custom"
});
enum_mapping!(log_level, parse_log_level, LogLevel, {
    LogLevel::Error => "error", LogLevel::Warn => "warn", LogLevel::Info => "info", LogLevel::Debug => "debug"
});
enum_mapping!(provider_kind, parse_provider_kind, ProviderKind, {
    ProviderKind::OpenAiCompatible => "open_ai_compatible", ProviderKind::OpenAi => "open_ai",
    ProviderKind::Ollama => "ollama", ProviderKind::LmStudio => "lm_studio",
    ProviderKind::Anthropic => "anthropic", ProviderKind::Gemini => "gemini"
});
enum_mapping!(privacy_class, parse_privacy_class, PrivacyClass, {
    PrivacyClass::Local => "local", PrivacyClass::Cloud => "cloud"
});
enum_mapping!(provider_status, parse_provider_status, ProviderStatus, {
    ProviderStatus::Configured => "configured", ProviderStatus::Connected => "connected",
    ProviderStatus::Unavailable => "unavailable", ProviderStatus::AuthenticationFailed => "authentication_failed",
    ProviderStatus::RateLimited => "rate_limited", ProviderStatus::EndpointUnreachable => "endpoint_unreachable",
    ProviderStatus::Unsupported => "unsupported"
});
enum_mapping!(ai_task, parse_ai_task, AiTaskType, {
    AiTaskType::Conversation => "conversation", AiTaskType::MemoryInterpretation => "memory_interpretation",
    AiTaskType::Consolidation => "consolidation"
});
enum_mapping!(chat_role, parse_chat_role, ChatRole, {
    ChatRole::User => "user", ChatRole::Assistant => "assistant", ChatRole::System => "system"
});
enum_mapping!(chat_origin, parse_chat_origin, ChatOrigin, {
    ChatOrigin::User => "user", ChatOrigin::Local => "local", ChatOrigin::Cache => "cache",
    ChatOrigin::Ai => "ai", ChatOrigin::System => "system"
});
enum_mapping!(chat_route, parse_chat_route, ChatRoute, {
    ChatRoute::LocalCapability => "local_capability", ChatRoute::LocalMemory => "local_memory",
    ChatRoute::ResponseCache => "response_cache", ChatRoute::Ai => "ai",
    ChatRoute::Clarification => "clarification", ChatRoute::Unavailable => "unavailable"
});
enum_mapping!(activity_kind, parse_activity_kind, ActivityKind, {
    ActivityKind::RequestReceived => "request_received", ActivityKind::LocalRouteHit => "local_route_hit",
    ActivityKind::LocalRouteMiss => "local_route_miss", ActivityKind::CacheHit => "cache_hit",
    ActivityKind::CacheMiss => "cache_miss", ActivityKind::AiSelected => "ai_selected",
    ActivityKind::AiCompleted => "ai_completed", ActivityKind::AiFailed => "ai_failed",
    ActivityKind::MemoryCommitted => "memory_committed", ActivityKind::PolicyEvaluated => "policy_evaluated",
    ActivityKind::ExecutionCompleted => "execution_completed", ActivityKind::ConfirmationRequired => "confirmation_required",
    ActivityKind::Error => "error"
});
enum_mapping!(memory_kind, parse_memory_kind, MemoryKind, {
    MemoryKind::Semantic => "semantic", MemoryKind::Procedural => "procedural"
});
enum_mapping!(trust_class, parse_trust_class, TrustClass, {
    TrustClass::UserExplicit => "user_explicit", TrustClass::LocalVerified => "local_verified",
    TrustClass::UserConfirmedResult => "user_confirmed_result", TrustClass::UserBehaviour => "user_behaviour",
    TrustClass::AiInferred => "ai_inferred", TrustClass::PluginAsserted => "plugin_asserted",
    TrustClass::ExternalContent => "external_content"
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_security_budgets_are_validated() {
        let mut database = Database::open_in_memory().unwrap();
        let mut settings = database.application_settings().unwrap();
        assert!(!settings.allow_cloud_ai);
        settings.allow_cloud_ai = true;
        settings.personal_instructions = "Keep answers concise".to_owned();
        database.update_application_settings(&settings, 10).unwrap();
        assert_eq!(database.application_settings().unwrap(), settings);

        settings.max_model_calls_per_request = 9;
        assert!(database.update_application_settings(&settings, 11).is_err());
    }

    #[test]
    fn provider_metadata_never_contains_a_secret() {
        let mut database = Database::open_in_memory().unwrap();
        let provider = Provider {
            id: ProviderId::generate(),
            kind: ProviderKind::OpenAiCompatible,
            name: "Local test".to_owned(),
            base_url: "http://127.0.0.1:11434/v1".to_owned(),
            enabled: true,
            privacy: PrivacyClass::Local,
            configured: true,
            credential_id: Some("credential:test".to_owned()),
            status: ProviderStatus::Configured,
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        database.upsert_provider(&provider).unwrap();
        let loaded = database.provider(&provider.id).unwrap().unwrap();
        assert_eq!(loaded.credential_id.as_deref(), Some("credential:test"));
        let serialized = serde_json::to_string(&loaded).unwrap();
        assert!(!serialized.contains("provider-secret-value"));
    }

    #[test]
    fn cache_requires_feedback_before_ai_response_reuse() {
        let mut database = Database::open_in_memory().unwrap();
        let entry = CachedResponse {
            id: CacheEntryId::generate(),
            normalized_request: "what is halquen".to_owned(),
            response: "A local-first personal system.".to_owned(),
            context_key: "global".to_owned(),
            confidence_permille: 550,
            priority_permille: 500,
            trust: TrustClass::AiInferred,
            valid_until_ms: None,
            reusable: false,
            created_at_ms: 1,
            last_used_at_ms: None,
            usage_count: 0,
            success_count: 0,
            correction_count: 0,
            original_provider_id: None,
            original_model_id: None,
            estimated_tokens_avoided: 0,
        };
        database.store_response_candidate(&entry).unwrap();
        assert!(
            database
                .cached_response("what is halquen", "global", 2)
                .unwrap()
                .is_none()
        );
        database
            .apply_response_feedback(&entry.id, ResponseFeedback::AlwaysUse)
            .unwrap();
        assert!(
            database
                .cached_response("what is halquen", "global", 2)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn stale_cache_is_rejected() {
        let mut database = Database::open_in_memory().unwrap();
        let entry = CachedResponse {
            id: CacheEntryId::generate(),
            normalized_request: "status".to_owned(),
            response: "old".to_owned(),
            context_key: "global".to_owned(),
            confidence_permille: 900,
            priority_permille: 500,
            trust: TrustClass::LocalVerified,
            valid_until_ms: Some(5),
            reusable: true,
            created_at_ms: 1,
            last_used_at_ms: None,
            usage_count: 0,
            success_count: 0,
            correction_count: 0,
            original_provider_id: None,
            original_model_id: None,
            estimated_tokens_avoided: 0,
        };
        database.store_response_candidate(&entry).unwrap();
        assert!(
            database
                .cached_response("status", "global", 6)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn replacing_a_candidate_replaces_its_feedback_identity_and_counters() {
        let mut database = Database::open_in_memory().unwrap();
        let first = CachedResponse {
            id: CacheEntryId::generate(),
            normalized_request: "same request".to_owned(),
            response: "first".to_owned(),
            context_key: "global".to_owned(),
            confidence_permille: 550,
            priority_permille: 500,
            trust: TrustClass::AiInferred,
            valid_until_ms: Some(100),
            reusable: false,
            created_at_ms: 1,
            last_used_at_ms: Some(2),
            usage_count: 4,
            success_count: 3,
            correction_count: 1,
            original_provider_id: None,
            original_model_id: None,
            estimated_tokens_avoided: 20,
        };
        database.store_response_candidate(&first).unwrap();
        let second = CachedResponse {
            id: CacheEntryId::generate(),
            response: "second".to_owned(),
            created_at_ms: 10,
            last_used_at_ms: None,
            usage_count: 0,
            success_count: 0,
            correction_count: 0,
            estimated_tokens_avoided: 0,
            ..first.clone()
        };
        database.store_response_candidate(&second).unwrap();

        assert!(
            database
                .apply_response_feedback(&first.id, ResponseFeedback::AlwaysUse)
                .is_err()
        );
        database
            .apply_response_feedback(&second.id, ResponseFeedback::AlwaysUse)
            .unwrap();
        let reused = database
            .cached_response("same request", "global", 11)
            .unwrap()
            .unwrap();
        assert_eq!(reused.id, second.id);
        assert_eq!(reused.response, "second");
        assert_eq!(reused.success_count, 1);
        assert_eq!(reused.estimated_tokens_avoided, 0);
    }
}
