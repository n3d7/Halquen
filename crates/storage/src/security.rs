use std::error::Error as StdError;

use halquen_domain::{
    ActionOrigin, ActionProposal, AgentConfiguration, AgentId, AgentResourceLimits, AgentTransport,
    BehaviourEventId, BehaviourOutcome, DataClassification, ExecutableIdentity,
    ExecutableOwnership, IntentUsageEvent, PermissionEffect, PermissionGrant, PermissionId,
    PermissionLifetime, PermissionScope, PermissionSessionScope, ResourceClassification,
    ResourceKind, ResourceLabel, ResourceLabelId, ResourceMatchKind, SandboxBackend,
    SecurityProfile,
};
use rusqlite::{OptionalExtension, Row, TransactionBehavior, params, types::Type};

use crate::{Database, StorageError};

impl Database {
    pub fn security_profile(&self) -> Result<SecurityProfile, StorageError> {
        self.connection
            .query_row(
                "SELECT profile FROM security_configuration WHERE singleton_id = 1",
                [],
                |row| parse_security_profile(&row.get::<_, String>(0)?),
            )
            .map_err(Into::into)
    }

    pub fn update_security_profile(
        &mut self,
        profile: SecurityProfile,
        now_ms: i64,
    ) -> Result<(), StorageError> {
        let updated = self.connection.execute(
            "UPDATE security_configuration SET profile = ?1, updated_at_ms = ?2
             WHERE singleton_id = 1",
            params![security_profile(profile), now_ms],
        )?;
        if updated != 1 {
            return Err(StorageError::InvalidInteraction(
                "security configuration singleton is missing".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn list_permission_grants(&self, limit: u16) -> Result<Vec<PermissionGrant>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, effect, lifetime, scope_json, session_kind, session_id, agent_id,
                    granted_by, granted_at_ms, expires_at_ms, revoked_at_ms, use_limit, use_count
             FROM permission_grants
             ORDER BY granted_at_ms DESC, id LIMIT ?1",
        )?;
        statement
            .query_map([i64::from(limit.min(200))], permission_from_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn active_permission_grants(
        &self,
        now_ms: i64,
        limit: u16,
    ) -> Result<Vec<PermissionGrant>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, effect, lifetime, scope_json, session_kind, session_id, agent_id,
                    granted_by, granted_at_ms, expires_at_ms, revoked_at_ms, use_limit, use_count
             FROM permission_grants
             WHERE revoked_at_ms IS NULL
               AND (expires_at_ms IS NULL OR expires_at_ms >= ?1)
               AND (use_limit IS NULL OR use_count < use_limit)
             ORDER BY granted_at_ms DESC, id LIMIT ?2",
        )?;
        statement
            .query_map(
                params![now_ms, i64::from(limit.min(200))],
                permission_from_row,
            )?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn upsert_permission_grant(&mut self, grant: &PermissionGrant) -> Result<(), StorageError> {
        grant
            .validate()
            .map_err(|_| StorageError::InvalidInteraction("invalid permission grant".to_owned()))?;
        let scope_json = serde_json::to_string(&grant.scope)?;
        if scope_json.len() > 32_768 {
            return Err(StorageError::InvalidInteraction(
                "permission scope exceeds 32768 bytes".to_owned(),
            ));
        }
        self.connection.execute(
            "INSERT INTO permission_grants(
                id, capability_id, scope_json, granted_at_ms, expires_at_ms, revoked_at_ms,
                effect, lifetime, session_kind, session_id, agent_id, granted_by, use_limit, use_count
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(id) DO UPDATE SET
                capability_id = excluded.capability_id,
                scope_json = excluded.scope_json,
                expires_at_ms = excluded.expires_at_ms,
                revoked_at_ms = excluded.revoked_at_ms,
                effect = excluded.effect,
                lifetime = excluded.lifetime,
                session_kind = excluded.session_kind,
                session_id = excluded.session_id,
                agent_id = excluded.agent_id,
                granted_by = excluded.granted_by,
                use_limit = excluded.use_limit,
                use_count = excluded.use_count",
            params![
                grant.id.as_str(),
                grant.scope.capability_id.as_str(),
                scope_json,
                grant.granted_at_ms,
                grant.expires_at_ms,
                grant.revoked_at_ms,
                permission_effect(grant.effect),
                permission_lifetime(grant.lifetime),
                grant.session.as_ref().map(permission_session_kind),
                grant.session.as_ref().map(permission_session_id),
                grant.agent_id.as_ref().map(|id| id.as_str()),
                action_origin(grant.granted_by),
                grant.use_limit,
                grant.use_count,
            ],
        )?;
        Ok(())
    }

    pub fn revoke_permission(
        &mut self,
        id: &PermissionId,
        now_ms: i64,
    ) -> Result<bool, StorageError> {
        Ok(self.connection.execute(
            "UPDATE permission_grants SET revoked_at_ms = ?1
             WHERE id = ?2 AND revoked_at_ms IS NULL",
            params![now_ms, id.as_str()],
        )? == 1)
    }

    pub fn consume_permission(&mut self, id: &PermissionId) -> Result<bool, StorageError> {
        Ok(self.connection.execute(
            "UPDATE permission_grants SET use_count = use_count + 1
             WHERE id = ?1 AND revoked_at_ms IS NULL
               AND (use_limit IS NULL OR use_count < use_limit)",
            [id.as_str()],
        )? == 1)
    }

    pub fn claim_permission_for_proposal(
        &mut self,
        proposal: &ActionProposal,
        session: Option<&PermissionSessionScope>,
        agent_id: Option<&AgentId>,
        now_ms: i64,
    ) -> Result<Option<PermissionId>, StorageError> {
        let scope_json = serde_json::to_string(&PermissionScope::from_proposal(proposal))?;
        let session_kind = session.map(permission_session_kind);
        let session_id = session.map(permission_session_id);
        let agent_id = agent_id.map(AgentId::as_str);
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let candidate: Option<String> = transaction
            .query_row(
                "SELECT id FROM permission_grants
                 WHERE effect = 'allow' AND capability_id = ?1 AND scope_json = ?2
                   AND revoked_at_ms IS NULL
                   AND (expires_at_ms IS NULL OR expires_at_ms >= ?3)
                   AND (use_limit IS NULL OR use_count < use_limit)
                   AND ((lifetime != 'session' AND session_kind IS NULL AND session_id IS NULL)
                        OR (lifetime = 'session' AND session_kind IS ?4 AND session_id IS ?5))
                   AND ((agent_id IS NULL AND ?6 IS NULL) OR agent_id = ?6)
                 ORDER BY granted_at_ms DESC, id LIMIT 1",
                params![
                    proposal.action.capability_id.as_str(),
                    scope_json,
                    now_ms,
                    session_kind,
                    session_id,
                    agent_id,
                ],
                |row| row.get(0),
            )
            .optional()?;
        let Some(candidate) = candidate else {
            transaction.commit()?;
            return Ok(None);
        };
        let consumed = transaction.execute(
            "UPDATE permission_grants SET use_count = use_count + 1
             WHERE id = ?1 AND revoked_at_ms IS NULL
               AND (expires_at_ms IS NULL OR expires_at_ms >= ?2)
               AND (use_limit IS NULL OR use_count < use_limit)",
            params![candidate, now_ms],
        )?;
        if consumed != 1 {
            return Err(StorageError::InvalidInteraction(
                "permission changed while it was being claimed".to_owned(),
            ));
        }
        transaction.commit()?;
        PermissionId::new(candidate)
            .map(Some)
            .map_err(|_| StorageError::InvalidInteraction("invalid permission id".to_owned()))
    }

    pub fn list_resource_labels(&self, limit: u16) -> Result<Vec<ResourceLabel>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, resource_kind, match_kind, pattern, classification,
                    data_classification, created_at_ms, updated_at_ms
             FROM resource_labels ORDER BY name COLLATE NOCASE, id LIMIT ?1",
        )?;
        statement
            .query_map([i64::from(limit.min(200))], resource_label_from_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn upsert_resource_label(&mut self, label: &ResourceLabel) -> Result<(), StorageError> {
        label
            .validate()
            .map_err(|_| StorageError::InvalidInteraction("invalid resource label".to_owned()))?;
        self.connection.execute(
            "INSERT INTO resource_labels(
                id, name, resource_kind, match_kind, pattern, classification,
                data_classification, created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                resource_kind = excluded.resource_kind,
                match_kind = excluded.match_kind,
                pattern = excluded.pattern,
                classification = excluded.classification,
                data_classification = excluded.data_classification,
                updated_at_ms = excluded.updated_at_ms",
            params![
                label.id.as_str(),
                label.name,
                resource_kind(label.resource_kind),
                resource_match_kind(label.match_kind),
                label.pattern,
                resource_classification(label.classification),
                data_classification(label.data_classification),
                label.created_at_ms,
                label.updated_at_ms,
            ],
        )?;
        Ok(())
    }

    pub fn resource_label_for(
        &self,
        kind: ResourceKind,
        identifier: &str,
    ) -> Result<Option<ResourceLabel>, StorageError> {
        let mut matches = self
            .list_resource_labels(200)?
            .into_iter()
            .filter(|label| {
                label.resource_kind == kind
                    && match label.match_kind {
                        ResourceMatchKind::Exact => identifier == label.pattern,
                        ResourceMatchKind::PathPrefix => {
                            identifier == label.pattern
                                || identifier
                                    .strip_prefix(&label.pattern)
                                    .is_some_and(|suffix| {
                                        label.pattern.ends_with('/') || suffix.starts_with('/')
                                    })
                        }
                        ResourceMatchKind::Host => {
                            identifier == label.pattern
                                || identifier
                                    .strip_suffix(&label.pattern)
                                    .is_some_and(|prefix| prefix.ends_with('.'))
                        }
                    }
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| match (left.match_kind, right.match_kind) {
            (ResourceMatchKind::Exact, ResourceMatchKind::Exact)
            | (ResourceMatchKind::PathPrefix, ResourceMatchKind::PathPrefix)
            | (ResourceMatchKind::Host, ResourceMatchKind::Host) => right
                .pattern
                .len()
                .cmp(&left.pattern.len())
                .then_with(|| left.id.cmp(&right.id)),
            (ResourceMatchKind::Exact, _) => std::cmp::Ordering::Less,
            (_, ResourceMatchKind::Exact) => std::cmp::Ordering::Greater,
            _ => right
                .pattern
                .len()
                .cmp(&left.pattern.len())
                .then_with(|| left.id.cmp(&right.id)),
        });
        Ok(matches.into_iter().next())
    }

    pub fn remove_resource_label(&mut self, id: &ResourceLabelId) -> Result<bool, StorageError> {
        Ok(self
            .connection
            .execute("DELETE FROM resource_labels WHERE id = ?1", [id.as_str()])?
            == 1)
    }

    pub fn record_intent_usage(&mut self, event: &IntentUsageEvent) -> Result<(), StorageError> {
        if event.intent.trim().is_empty()
            || event.intent.len() > 128
            || event.context_class.trim().is_empty()
            || event.context_class.len() > 128
        {
            return Err(StorageError::InvalidInteraction(
                "intent usage event is outside accepted bounds".to_owned(),
            ));
        }
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO intent_usage_events(
                id, intent, entity_id, outcome, context_class, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event.id.as_str(),
                event.intent,
                event.entity_id.as_str(),
                behaviour_outcome(event.outcome),
                event.context_class,
                event.created_at_ms,
            ],
        )?;
        let cutoff = event
            .created_at_ms
            .saturating_sub(halquen_memory::DEFAULT_RETENTION_MS);
        transaction.execute(
            "DELETE FROM intent_usage_events WHERE created_at_ms < ?1",
            [cutoff],
        )?;
        transaction.execute(
            "DELETE FROM intent_usage_events WHERE id IN (
                SELECT id FROM intent_usage_events
                WHERE intent = ?1 AND context_class = ?2
                ORDER BY created_at_ms DESC, id DESC LIMIT -1 OFFSET ?3
             )",
            params![
                event.intent,
                event.context_class,
                i64::try_from(halquen_memory::DEFAULT_MAX_EVENTS).unwrap_or(512),
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn recent_intent_usage(
        &self,
        intent: &str,
        context_class: &str,
        since_ms: i64,
        limit: u16,
    ) -> Result<Vec<IntentUsageEvent>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, intent, entity_id, outcome, context_class, created_at_ms
             FROM intent_usage_events
             WHERE intent = ?1 AND context_class = ?2 AND created_at_ms >= ?3
             ORDER BY created_at_ms DESC, id DESC LIMIT ?4",
        )?;
        statement
            .query_map(
                params![intent, context_class, since_ms, i64::from(limit.min(512))],
                intent_usage_from_row,
            )?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn list_agents(&self, limit: u16) -> Result<Vec<AgentConfiguration>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, transport, executable, arguments_json, socket_path, sandbox,
                    executable_ownership, executable_device, executable_inode,
                    executable_owner_uid, executable_size, executable_mtime_seconds,
                    executable_mtime_nanoseconds, executable_sha256_hex,
                    cpu_seconds, memory_bytes, process_count, file_size_bytes, open_files,
                    temp_bytes, enabled, timeout_ms, max_stdout_bytes, max_stderr_bytes,
                    created_at_ms, updated_at_ms
             FROM agent_configurations ORDER BY name COLLATE NOCASE, id LIMIT ?1",
        )?;
        statement
            .query_map([i64::from(limit.min(100))], agent_from_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn upsert_agent(&mut self, agent: &AgentConfiguration) -> Result<(), StorageError> {
        agent.validate().map_err(|_| {
            StorageError::InvalidInteraction("invalid agent configuration".to_owned())
        })?;
        let arguments = serde_json::to_string(&agent.arguments)?;
        let timeout_ms = i64::try_from(agent.timeout_ms).map_err(|_| {
            StorageError::InvalidInteraction("agent timeout exceeds SQLite range".to_owned())
        })?;
        let (device, inode, owner_uid, size, mtime_seconds, mtime_nanoseconds, sha256_hex) =
            match &agent.executable_identity {
                Some(identity) => (
                    Some(to_sqlite_u64(identity.device)?),
                    Some(to_sqlite_u64(identity.inode)?),
                    Some(i64::from(identity.owner_uid)),
                    Some(to_sqlite_u64(identity.size)?),
                    Some(identity.modified_seconds),
                    Some(identity.modified_nanoseconds),
                    identity.sha256_hex.as_deref(),
                ),
                None => (None, None, None, None, None, None, None),
            };
        self.connection.execute(
            "INSERT INTO agent_configurations(
                id, name, transport, executable, arguments_json, socket_path, sandbox,
                executable_ownership, executable_device, executable_inode,
                executable_owner_uid, executable_size, executable_mtime_seconds,
                executable_mtime_nanoseconds, executable_sha256_hex,
                cpu_seconds, memory_bytes, process_count, file_size_bytes, open_files,
                temp_bytes, enabled, timeout_ms, max_stdout_bytes, max_stderr_bytes,
                created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                       ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                transport = excluded.transport,
                executable = excluded.executable,
                arguments_json = excluded.arguments_json,
                socket_path = excluded.socket_path,
                sandbox = excluded.sandbox,
                executable_ownership = excluded.executable_ownership,
                executable_device = excluded.executable_device,
                executable_inode = excluded.executable_inode,
                executable_owner_uid = excluded.executable_owner_uid,
                executable_size = excluded.executable_size,
                executable_mtime_seconds = excluded.executable_mtime_seconds,
                executable_mtime_nanoseconds = excluded.executable_mtime_nanoseconds,
                executable_sha256_hex = excluded.executable_sha256_hex,
                cpu_seconds = excluded.cpu_seconds,
                memory_bytes = excluded.memory_bytes,
                process_count = excluded.process_count,
                file_size_bytes = excluded.file_size_bytes,
                open_files = excluded.open_files,
                temp_bytes = excluded.temp_bytes,
                enabled = excluded.enabled,
                timeout_ms = excluded.timeout_ms,
                max_stdout_bytes = excluded.max_stdout_bytes,
                max_stderr_bytes = excluded.max_stderr_bytes,
                updated_at_ms = excluded.updated_at_ms",
            params![
                agent.id.as_str(),
                agent.name,
                agent_transport(agent.transport),
                agent.executable,
                arguments,
                agent.socket_path,
                sandbox_backend(agent.sandbox),
                executable_ownership(agent.ownership),
                device,
                inode,
                owner_uid,
                size,
                mtime_seconds,
                mtime_nanoseconds,
                sha256_hex,
                agent.resource_limits.cpu_seconds,
                to_sqlite_u64(agent.resource_limits.memory_bytes)?,
                agent.resource_limits.process_count,
                to_sqlite_u64(agent.resource_limits.file_size_bytes)?,
                agent.resource_limits.open_files,
                to_sqlite_u64(agent.resource_limits.temp_bytes)?,
                agent.enabled,
                timeout_ms,
                agent.max_stdout_bytes,
                agent.max_stderr_bytes,
                agent.created_at_ms,
                agent.updated_at_ms,
            ],
        )?;
        Ok(())
    }

    pub fn remove_agent(&mut self, id: &AgentId) -> Result<bool, StorageError> {
        Ok(self.connection.execute(
            "DELETE FROM agent_configurations WHERE id = ?1",
            [id.as_str()],
        )? == 1)
    }
}

fn permission_from_row(row: &Row<'_>) -> rusqlite::Result<PermissionGrant> {
    let scope_json: String = row.get(3)?;
    let scope: PermissionScope = serde_json::from_str(&scope_json).map_err(conversion_error)?;
    Ok(PermissionGrant {
        id: PermissionId::new(row.get::<_, String>(0)?).map_err(conversion_error)?,
        effect: parse_permission_effect(&row.get::<_, String>(1)?)?,
        lifetime: parse_permission_lifetime(&row.get::<_, String>(2)?)?,
        scope,
        session: parse_permission_session(row.get(4)?, row.get(5)?)?,
        agent_id: row
            .get::<_, Option<String>>(6)?
            .map(AgentId::new)
            .transpose()
            .map_err(conversion_error)?,
        granted_by: parse_action_origin(&row.get::<_, String>(7)?)?,
        granted_at_ms: row.get(8)?,
        expires_at_ms: row.get(9)?,
        revoked_at_ms: row.get(10)?,
        use_limit: row.get(11)?,
        use_count: row.get(12)?,
    })
}

fn permission_session_kind(value: &PermissionSessionScope) -> &'static str {
    match value {
        PermissionSessionScope::Chat(_) => "chat",
        PermissionSessionScope::Agent(_) => "agent",
        PermissionSessionScope::Daemon(_) => "daemon",
    }
}

fn permission_session_id(value: &PermissionSessionScope) -> &str {
    match value {
        PermissionSessionScope::Chat(id) => id.as_str(),
        PermissionSessionScope::Agent(id) => id.as_str(),
        PermissionSessionScope::Daemon(id) => id.as_str(),
    }
}

fn parse_permission_session(
    kind: Option<String>,
    id: Option<String>,
) -> rusqlite::Result<Option<PermissionSessionScope>> {
    match (kind.as_deref(), id) {
        (None, None) => Ok(None),
        (Some("chat"), Some(id)) => halquen_domain::ChatSessionId::new(id)
            .map(PermissionSessionScope::Chat)
            .map(Some)
            .map_err(conversion_error),
        (Some("agent"), Some(id)) => halquen_domain::AgentSessionId::new(id)
            .map(PermissionSessionScope::Agent)
            .map(Some)
            .map_err(conversion_error),
        (Some("daemon"), Some(id)) => halquen_domain::DaemonSessionId::new(id)
            .map(PermissionSessionScope::Daemon)
            .map(Some)
            .map_err(conversion_error),
        _ => Err(conversion_error(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid permission session scope",
        ))),
    }
}

fn resource_label_from_row(row: &Row<'_>) -> rusqlite::Result<ResourceLabel> {
    Ok(ResourceLabel {
        id: ResourceLabelId::new(row.get::<_, String>(0)?).map_err(conversion_error)?,
        name: row.get(1)?,
        resource_kind: parse_resource_kind(&row.get::<_, String>(2)?)?,
        match_kind: parse_resource_match_kind(&row.get::<_, String>(3)?)?,
        pattern: row.get(4)?,
        classification: parse_resource_classification(&row.get::<_, String>(5)?)?,
        data_classification: parse_data_classification(&row.get::<_, String>(6)?)?,
        created_at_ms: row.get(7)?,
        updated_at_ms: row.get(8)?,
    })
}

fn intent_usage_from_row(row: &Row<'_>) -> rusqlite::Result<IntentUsageEvent> {
    Ok(IntentUsageEvent {
        id: BehaviourEventId::new(row.get::<_, String>(0)?).map_err(conversion_error)?,
        intent: row.get(1)?,
        entity_id: halquen_domain::EntityId::new(row.get::<_, String>(2)?)
            .map_err(conversion_error)?,
        outcome: parse_behaviour_outcome(&row.get::<_, String>(3)?)?,
        context_class: row.get(4)?,
        created_at_ms: row.get(5)?,
    })
}

fn agent_from_row(row: &Row<'_>) -> rusqlite::Result<AgentConfiguration> {
    let arguments: String = row.get(4)?;
    let executable: String = row.get(3)?;
    let identity = match row.get::<_, Option<i64>>(8)? {
        Some(device) => Some(ExecutableIdentity {
            canonical_path: executable.clone(),
            device: u64::try_from(device).map_err(conversion_error)?,
            inode: u64::try_from(row.get::<_, i64>(9)?).map_err(conversion_error)?,
            owner_uid: u32::try_from(row.get::<_, i64>(10)?).map_err(conversion_error)?,
            size: u64::try_from(row.get::<_, i64>(11)?).map_err(conversion_error)?,
            modified_seconds: row.get(12)?,
            modified_nanoseconds: row.get(13)?,
            sha256_hex: row.get(14)?,
        }),
        None => None,
    };
    let timeout_ms = u64::try_from(row.get::<_, i64>(22)?).map_err(conversion_error)?;
    Ok(AgentConfiguration {
        id: AgentId::new(row.get::<_, String>(0)?).map_err(conversion_error)?,
        name: row.get(1)?,
        transport: parse_agent_transport(&row.get::<_, String>(2)?)?,
        executable,
        arguments: serde_json::from_str(&arguments).map_err(conversion_error)?,
        socket_path: row.get(5)?,
        sandbox: parse_sandbox_backend(&row.get::<_, String>(6)?)?,
        ownership: parse_executable_ownership(&row.get::<_, String>(7)?)?,
        executable_identity: identity,
        resource_limits: AgentResourceLimits {
            cpu_seconds: row.get(15)?,
            memory_bytes: u64::try_from(row.get::<_, i64>(16)?).map_err(conversion_error)?,
            process_count: row.get(17)?,
            file_size_bytes: u64::try_from(row.get::<_, i64>(18)?).map_err(conversion_error)?,
            open_files: row.get(19)?,
            temp_bytes: u64::try_from(row.get::<_, i64>(20)?).map_err(conversion_error)?,
        },
        enabled: row.get(21)?,
        timeout_ms,
        max_stdout_bytes: row.get(23)?,
        max_stderr_bytes: row.get(24)?,
        created_at_ms: row.get(25)?,
        updated_at_ms: row.get(26)?,
    })
}

fn conversion_error(error: impl StdError + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(error))
}

fn to_sqlite_u64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| {
        StorageError::InvalidInteraction("numeric value exceeds SQLite range".to_owned())
    })
}

macro_rules! enum_codec {
    ($encode:ident, $parse:ident, $ty:ty, {$($variant:path => $value:literal),+ $(,)?}) => {
        fn $encode(value: $ty) -> &'static str {
            match value { $($variant => $value),+ }
        }
        fn $parse(value: &str) -> rusqlite::Result<$ty> {
            match value {
                $($value => Ok($variant)),+,
                _ => Err(conversion_error(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid persisted enum value {value}"),
                ))),
            }
        }
    };
}

enum_codec!(security_profile, parse_security_profile, SecurityProfile, {
    SecurityProfile::Strict => "strict", SecurityProfile::Balanced => "balanced",
    SecurityProfile::Developer => "developer", SecurityProfile::Custom => "custom"
});
enum_codec!(permission_effect, parse_permission_effect, PermissionEffect, {
    PermissionEffect::Allow => "allow", PermissionEffect::Deny => "deny"
});
enum_codec!(permission_lifetime, parse_permission_lifetime, PermissionLifetime, {
    PermissionLifetime::Once => "once", PermissionLifetime::Session => "session",
    PermissionLifetime::Until => "until", PermissionLifetime::Always => "always"
});
enum_codec!(action_origin, parse_action_origin, ActionOrigin, {
    ActionOrigin::UserExplicit => "user_explicit", ActionOrigin::System => "system",
    ActionOrigin::LocalResolver => "local_resolver", ActionOrigin::AiProposal => "ai_proposal",
    ActionOrigin::ExternalContent => "external_content", ActionOrigin::Plugin => "plugin",
    ActionOrigin::StoredProcedure => "stored_procedure", ActionOrigin::Agent => "agent"
});
enum_codec!(resource_kind, parse_resource_kind, ResourceKind, {
    ResourceKind::Application => "application", ResourceKind::File => "file",
    ResourceKind::NetworkEndpoint => "network_endpoint", ResourceKind::Database => "database",
    ResourceKind::Agent => "agent", ResourceKind::Plugin => "plugin", ResourceKind::System => "system"
});
enum_codec!(resource_match_kind, parse_resource_match_kind, ResourceMatchKind, {
    ResourceMatchKind::Exact => "exact", ResourceMatchKind::PathPrefix => "path_prefix",
    ResourceMatchKind::Host => "host"
});
enum_codec!(resource_classification, parse_resource_classification, ResourceClassification, {
    ResourceClassification::Public => "public", ResourceClassification::Local => "local",
    ResourceClassification::Personal => "personal", ResourceClassification::Sensitive => "sensitive",
    ResourceClassification::Secret => "secret", ResourceClassification::Production => "production",
    ResourceClassification::SystemCritical => "system_critical"
});
enum_codec!(data_classification, parse_data_classification, DataClassification, {
    DataClassification::Public => "public", DataClassification::Personal => "personal",
    DataClassification::Sensitive => "sensitive", DataClassification::Secret => "secret",
    DataClassification::Production => "production"
});
enum_codec!(behaviour_outcome, parse_behaviour_outcome, BehaviourOutcome, {
    BehaviourOutcome::Success => "success", BehaviourOutcome::Failure => "failure",
    BehaviourOutcome::CorrectionAccepted => "correction_accepted",
    BehaviourOutcome::CorrectionRejected => "correction_rejected"
});
enum_codec!(agent_transport, parse_agent_transport, AgentTransport, {
    AgentTransport::Cli => "cli", AgentTransport::UnixSocket => "unix_socket"
});
enum_codec!(sandbox_backend, parse_sandbox_backend, SandboxBackend, {
    SandboxBackend::Bubblewrap => "bubblewrap", SandboxBackend::Unavailable => "unavailable",
    SandboxBackend::UnsafeUnsandboxed => "unsafe_unsandboxed"
});
enum_codec!(executable_ownership, parse_executable_ownership, ExecutableOwnership, {
    ExecutableOwnership::RootOnly => "root_only",
    ExecutableOwnership::RootOrCurrentUser => "root_or_current_user"
});

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::{Arc, Barrier};

    use halquen_domain::{
        ActionArguments, ActionContext, ActionProposal, ActionRequest, CapabilityId, EntityId,
        PermissionScope, ResourceDescriptor,
    };

    use super::*;

    #[test]
    fn exact_permissions_are_persisted_revoked_and_consumed() {
        let mut database = Database::open_in_memory().unwrap();
        let grant = PermissionGrant {
            id: PermissionId::generate(),
            effect: PermissionEffect::Allow,
            lifetime: PermissionLifetime::Once,
            scope: PermissionScope {
                capability_id: CapabilityId::new("system.open_app").unwrap(),
                arguments: ActionArguments::OpenApp {
                    app: EntityId::new("app:telegram").unwrap(),
                },
                resources: vec![ResourceDescriptor {
                    kind: ResourceKind::Application,
                    identifier: "app:telegram".to_owned(),
                    classification: ResourceClassification::Local,
                }],
                destination: None,
            },
            session: None,
            agent_id: None,
            granted_by: ActionOrigin::UserExplicit,
            granted_at_ms: 10,
            expires_at_ms: None,
            revoked_at_ms: None,
            use_limit: Some(1),
            use_count: 0,
        };
        database.upsert_permission_grant(&grant).unwrap();
        assert_eq!(
            database.active_permission_grants(10, 10).unwrap(),
            vec![grant.clone()]
        );
        assert!(database.consume_permission(&grant.id).unwrap());
        assert!(
            database
                .active_permission_grants(10, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn behaviour_storage_is_bounded() {
        let mut database = Database::open_in_memory().unwrap();
        for timestamp in 0..520_i64 {
            database
                .record_intent_usage(&IntentUsageEvent {
                    id: BehaviourEventId::generate(),
                    intent: "open_application".to_owned(),
                    entity_id: halquen_domain::EntityId::new("app:telegram").unwrap(),
                    outcome: BehaviourOutcome::Success,
                    context_class: "application".to_owned(),
                    created_at_ms: timestamp,
                })
                .unwrap();
        }
        assert_eq!(
            database
                .recent_intent_usage("open_application", "application", 0, 512)
                .unwrap()
                .len(),
            512
        );
    }

    #[test]
    fn most_specific_resource_label_wins() {
        let mut database = Database::open_in_memory().unwrap();
        for (name, pattern, classification) in [
            (
                "documents",
                "/home/fixture/Documents",
                ResourceClassification::Personal,
            ),
            (
                "production",
                "/home/fixture/Documents/production",
                ResourceClassification::Production,
            ),
        ] {
            database
                .upsert_resource_label(&ResourceLabel {
                    id: ResourceLabelId::generate(),
                    name: name.to_owned(),
                    resource_kind: ResourceKind::File,
                    match_kind: ResourceMatchKind::PathPrefix,
                    pattern: pattern.to_owned(),
                    classification,
                    data_classification: DataClassification::Sensitive,
                    created_at_ms: 1,
                    updated_at_ms: 1,
                })
                .unwrap();
        }
        let label = database
            .resource_label_for(
                ResourceKind::File,
                "/home/fixture/Documents/production/db.dump",
            )
            .unwrap()
            .unwrap();
        assert_eq!(label.classification, ResourceClassification::Production);
    }

    #[test]
    fn concurrent_once_permission_claim_has_exactly_one_winner() {
        let directory = std::env::temp_dir().join(format!(
            "halquen-once-grant-race-{}-{}",
            std::process::id(),
            PermissionId::generate()
        ));
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let path = directory.join("halquen.sqlite3");
        let capability_id = CapabilityId::new("system.open_app").unwrap();
        let app = EntityId::new("app:race_fixture").unwrap();
        let resource = ResourceDescriptor {
            kind: ResourceKind::Application,
            identifier: app.to_string(),
            classification: ResourceClassification::Local,
        };
        let proposal = ActionProposal::new(
            ActionRequest::new(capability_id, ActionArguments::OpenApp { app }),
            ActionContext::trusted_user(None).with_resource(resource),
        )
        .unwrap();
        {
            let mut database = Database::open(&path).unwrap();
            database
                .upsert_permission_grant(&PermissionGrant {
                    id: PermissionId::generate(),
                    effect: PermissionEffect::Allow,
                    lifetime: PermissionLifetime::Once,
                    scope: PermissionScope::from_proposal(&proposal),
                    session: None,
                    agent_id: None,
                    granted_by: ActionOrigin::UserExplicit,
                    granted_at_ms: 1,
                    expires_at_ms: None,
                    revoked_at_ms: None,
                    use_limit: Some(1),
                    use_count: 0,
                })
                .unwrap();
        }

        let barrier = Arc::new(Barrier::new(2));
        let handles = (0..2)
            .map(|_| {
                let path = PathBuf::from(&path);
                let proposal = proposal.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let mut database = Database::open(&path).unwrap();
                    barrier.wait();
                    database
                        .claim_permission_for_proposal(&proposal, None, None, 2)
                        .unwrap()
                        .is_some()
                })
            })
            .collect::<Vec<_>>();
        let winners = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .filter(|winner| *winner)
            .count();
        assert_eq!(winners, 1);
        fs::remove_dir_all(directory).unwrap();
    }
}
