use std::error::Error as StdError;

use halquen_domain::{
    AgentId, AgentInstanceId, AgentSession, AgentSessionId, AgentSessionStatus, DaemonSession,
    DaemonSessionId, EntityId, ExecutableIdentity, ExecutableOwnership, RegisteredApplication,
};
use rusqlite::{Row, params, types::Type};

use crate::{Database, StorageError};

impl Database {
    pub fn list_registered_applications(
        &self,
        limit: u16,
    ) -> Result<Vec<RegisteredApplication>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT entity_id, display_name, executable, arguments_json, executable_ownership,
                    executable_device, executable_inode, executable_owner_uid, executable_size,
                    executable_mtime_seconds, executable_mtime_nanoseconds,
                    executable_sha256_hex, enabled, created_at_ms, updated_at_ms
             FROM registered_applications
             ORDER BY display_name COLLATE NOCASE, entity_id LIMIT ?1",
        )?;
        statement
            .query_map([i64::from(limit.min(200))], application_from_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn upsert_registered_application(
        &mut self,
        application: &RegisteredApplication,
    ) -> Result<(), StorageError> {
        application.validate().map_err(|_| {
            StorageError::InvalidInteraction("invalid application registration".to_owned())
        })?;
        self.connection.execute(
            "INSERT INTO registered_applications(
                entity_id, display_name, executable, arguments_json, executable_ownership,
                executable_device, executable_inode, executable_owner_uid, executable_size,
                executable_mtime_seconds, executable_mtime_nanoseconds,
                executable_sha256_hex, enabled, created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(entity_id) DO UPDATE SET
                display_name = excluded.display_name,
                executable = excluded.executable,
                arguments_json = excluded.arguments_json,
                executable_ownership = excluded.executable_ownership,
                executable_device = excluded.executable_device,
                executable_inode = excluded.executable_inode,
                executable_owner_uid = excluded.executable_owner_uid,
                executable_size = excluded.executable_size,
                executable_mtime_seconds = excluded.executable_mtime_seconds,
                executable_mtime_nanoseconds = excluded.executable_mtime_nanoseconds,
                executable_sha256_hex = excluded.executable_sha256_hex,
                enabled = excluded.enabled,
                updated_at_ms = excluded.updated_at_ms",
            params![
                application.entity_id.as_str(),
                application.display_name,
                application.executable,
                serde_json::to_string(&application.arguments)?,
                executable_ownership(application.ownership),
                to_sqlite_u64(application.identity.device)?,
                to_sqlite_u64(application.identity.inode)?,
                i64::from(application.identity.owner_uid),
                to_sqlite_u64(application.identity.size)?,
                application.identity.modified_seconds,
                application.identity.modified_nanoseconds,
                application.identity.sha256_hex,
                application.enabled,
                application.created_at_ms,
                application.updated_at_ms,
            ],
        )?;
        Ok(())
    }

    pub fn remove_registered_application(
        &mut self,
        entity_id: &EntityId,
    ) -> Result<bool, StorageError> {
        Ok(self.connection.execute(
            "DELETE FROM registered_applications WHERE entity_id = ?1",
            [entity_id.as_str()],
        )? == 1)
    }

    pub fn begin_daemon_session(&mut self, session: &DaemonSession) -> Result<(), StorageError> {
        if session.ended_at_ms.is_some() {
            return Err(StorageError::InvalidInteraction(
                "new daemon session is already ended".to_owned(),
            ));
        }
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "UPDATE agent_sessions
             SET status = 'crashed', ended_at_ms = MAX(started_at_ms, ?1)
             WHERE status = 'running' AND ended_at_ms IS NULL",
            [session.started_at_ms],
        )?;
        transaction.execute(
            "UPDATE daemon_sessions
             SET ended_at_ms = MAX(started_at_ms, ?1)
             WHERE ended_at_ms IS NULL",
            [session.started_at_ms],
        )?;
        transaction.execute(
            "UPDATE permission_grants SET revoked_at_ms = ?1
             WHERE revoked_at_ms IS NULL AND lifetime = 'session'
               AND session_kind IN ('agent', 'daemon')",
            [session.started_at_ms],
        )?;
        transaction.execute(
            "INSERT INTO daemon_sessions(id, started_at_ms, ended_at_ms) VALUES (?1, ?2, NULL)",
            params![session.id.as_str(), session.started_at_ms],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn finish_daemon_session(
        &mut self,
        id: &DaemonSessionId,
        ended_at_ms: i64,
    ) -> Result<bool, StorageError> {
        let transaction = self.connection.transaction()?;
        let updated = transaction.execute(
            "UPDATE daemon_sessions SET ended_at_ms = MAX(started_at_ms, ?1)
             WHERE id = ?2 AND ended_at_ms IS NULL",
            params![ended_at_ms, id.as_str()],
        )? == 1;
        transaction.execute(
            "UPDATE permission_grants SET revoked_at_ms = ?1
             WHERE revoked_at_ms IS NULL AND lifetime = 'session'
               AND session_kind = 'daemon' AND session_id = ?2",
            params![ended_at_ms, id.as_str()],
        )?;
        transaction.commit()?;
        Ok(updated)
    }

    pub fn begin_agent_session(&mut self, session: &AgentSession) -> Result<(), StorageError> {
        if session.status != AgentSessionStatus::Running || session.ended_at_ms.is_some() {
            return Err(StorageError::InvalidInteraction(
                "new agent session must be running".to_owned(),
            ));
        }
        self.connection.execute(
            "INSERT INTO agent_sessions(
                id, agent_id, instance_id, daemon_session_id, status, started_at_ms, ended_at_ms
             ) VALUES (?1, ?2, ?3, ?4, 'running', ?5, NULL)",
            params![
                session.id.as_str(),
                session.agent_id.as_str(),
                session.instance_id.as_str(),
                session.daemon_session_id.as_str(),
                session.started_at_ms,
            ],
        )?;
        Ok(())
    }

    pub fn finish_agent_session(
        &mut self,
        id: &AgentSessionId,
        status: AgentSessionStatus,
        ended_at_ms: i64,
    ) -> Result<bool, StorageError> {
        if status == AgentSessionStatus::Running {
            return Err(StorageError::InvalidInteraction(
                "finished agent session cannot remain running".to_owned(),
            ));
        }
        let transaction = self.connection.transaction()?;
        let updated = transaction.execute(
            "UPDATE agent_sessions
             SET status = ?1, ended_at_ms = MAX(started_at_ms, ?2)
             WHERE id = ?3 AND status = 'running' AND ended_at_ms IS NULL",
            params![agent_session_status(status), ended_at_ms, id.as_str()],
        )? == 1;
        transaction.execute(
            "UPDATE permission_grants SET revoked_at_ms = ?1
             WHERE revoked_at_ms IS NULL AND lifetime = 'session'
               AND session_kind = 'agent' AND session_id = ?2",
            params![ended_at_ms, id.as_str()],
        )?;
        transaction.commit()?;
        Ok(updated)
    }

    pub fn list_agent_sessions(&self, limit: u16) -> Result<Vec<AgentSession>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, agent_id, instance_id, daemon_session_id, status,
                    started_at_ms, ended_at_ms
             FROM agent_sessions ORDER BY started_at_ms DESC, id DESC LIMIT ?1",
        )?;
        statement
            .query_map([i64::from(limit.min(200))], agent_session_from_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}

fn application_from_row(row: &Row<'_>) -> rusqlite::Result<RegisteredApplication> {
    let arguments: String = row.get(3)?;
    Ok(RegisteredApplication {
        entity_id: EntityId::new(row.get::<_, String>(0)?).map_err(conversion_error)?,
        display_name: row.get(1)?,
        executable: row.get(2)?,
        arguments: serde_json::from_str(&arguments).map_err(conversion_error)?,
        ownership: parse_executable_ownership(&row.get::<_, String>(4)?)?,
        identity: ExecutableIdentity {
            canonical_path: row.get(2)?,
            device: from_sqlite_u64(row.get(5)?)?,
            inode: from_sqlite_u64(row.get(6)?)?,
            owner_uid: u32::try_from(row.get::<_, i64>(7)?).map_err(conversion_error)?,
            size: from_sqlite_u64(row.get(8)?)?,
            modified_seconds: row.get(9)?,
            modified_nanoseconds: row.get(10)?,
            sha256_hex: row.get(11)?,
        },
        enabled: row.get(12)?,
        created_at_ms: row.get(13)?,
        updated_at_ms: row.get(14)?,
    })
}

fn agent_session_from_row(row: &Row<'_>) -> rusqlite::Result<AgentSession> {
    Ok(AgentSession {
        id: AgentSessionId::new(row.get::<_, String>(0)?).map_err(conversion_error)?,
        agent_id: AgentId::new(row.get::<_, String>(1)?).map_err(conversion_error)?,
        instance_id: AgentInstanceId::new(row.get::<_, String>(2)?).map_err(conversion_error)?,
        daemon_session_id: DaemonSessionId::new(row.get::<_, String>(3)?)
            .map_err(conversion_error)?,
        status: parse_agent_session_status(&row.get::<_, String>(4)?)?,
        started_at_ms: row.get(5)?,
        ended_at_ms: row.get(6)?,
    })
}

fn executable_ownership(value: ExecutableOwnership) -> &'static str {
    match value {
        ExecutableOwnership::RootOnly => "root_only",
        ExecutableOwnership::RootOrCurrentUser => "root_or_current_user",
    }
}

fn parse_executable_ownership(value: &str) -> rusqlite::Result<ExecutableOwnership> {
    match value {
        "root_only" => Ok(ExecutableOwnership::RootOnly),
        "root_or_current_user" => Ok(ExecutableOwnership::RootOrCurrentUser),
        _ => Err(conversion_error(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid executable ownership",
        ))),
    }
}

fn agent_session_status(value: AgentSessionStatus) -> &'static str {
    match value {
        AgentSessionStatus::Running => "running",
        AgentSessionStatus::Completed => "completed",
        AgentSessionStatus::Failed => "failed",
        AgentSessionStatus::TimedOut => "timed_out",
        AgentSessionStatus::Crashed => "crashed",
    }
}

fn parse_agent_session_status(value: &str) -> rusqlite::Result<AgentSessionStatus> {
    match value {
        "running" => Ok(AgentSessionStatus::Running),
        "completed" => Ok(AgentSessionStatus::Completed),
        "failed" => Ok(AgentSessionStatus::Failed),
        "timed_out" => Ok(AgentSessionStatus::TimedOut),
        "crashed" => Ok(AgentSessionStatus::Crashed),
        _ => Err(conversion_error(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid agent session status",
        ))),
    }
}

fn to_sqlite_u64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| {
        StorageError::InvalidInteraction("numeric identity exceeds SQLite range".to_owned())
    })
}

fn from_sqlite_u64(value: i64) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(conversion_error)
}

fn conversion_error(error: impl StdError + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(error))
}

#[cfg(test)]
mod tests {
    use halquen_domain::{
        ActionArguments, ActionOrigin, CapabilityId, PermissionEffect, PermissionGrant,
        PermissionId, PermissionLifetime, PermissionScope, PermissionSessionScope,
    };

    use super::*;

    #[test]
    fn daemon_restart_crashes_stale_agent_sessions_and_revokes_session_grants() {
        let mut database = Database::open_in_memory().unwrap();
        let first_daemon = DaemonSession {
            id: DaemonSessionId::generate(),
            started_at_ms: 10,
            ended_at_ms: None,
        };
        database.begin_daemon_session(&first_daemon).unwrap();
        let agent_id = AgentId::generate();
        let agent_session = AgentSession {
            id: AgentSessionId::generate(),
            agent_id: agent_id.clone(),
            instance_id: AgentInstanceId::generate(),
            daemon_session_id: first_daemon.id.clone(),
            status: AgentSessionStatus::Running,
            started_at_ms: 20,
            ended_at_ms: None,
        };
        database.begin_agent_session(&agent_session).unwrap();
        database
            .upsert_permission_grant(&PermissionGrant {
                id: PermissionId::generate(),
                effect: PermissionEffect::Allow,
                lifetime: PermissionLifetime::Session,
                scope: PermissionScope {
                    capability_id: CapabilityId::new("system.open_app").unwrap(),
                    arguments: ActionArguments::OpenApp {
                        app: EntityId::new("app:test").unwrap(),
                    },
                    resources: Vec::new(),
                    destination: None,
                },
                session: Some(PermissionSessionScope::Agent(agent_session.id.clone())),
                agent_id: Some(agent_id),
                granted_by: ActionOrigin::UserExplicit,
                granted_at_ms: 21,
                expires_at_ms: None,
                revoked_at_ms: None,
                use_limit: None,
                use_count: 0,
            })
            .unwrap();

        database
            .begin_daemon_session(&DaemonSession {
                id: DaemonSessionId::generate(),
                started_at_ms: 30,
                ended_at_ms: None,
            })
            .unwrap();

        let sessions = database.list_agent_sessions(10).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].status, AgentSessionStatus::Crashed);
        assert_eq!(sessions[0].ended_at_ms, Some(30));
        assert!(
            database
                .active_permission_grants(30, 10)
                .unwrap()
                .is_empty()
        );
    }
}
