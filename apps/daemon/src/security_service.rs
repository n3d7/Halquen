use halquen_capabilities::{Executor, inspect_executable};
use halquen_domain::{
    ActionOrigin, AgentId, AgentSessionStatus, EntityId, PermissionGrant, PermissionId,
    PermissionLifetime, PermissionScope, PermissionSessionScope, RegisteredApplication,
    ResourceLabel, ResourceLabelId, SecurityProfile,
};
use halquen_protocol::{
    AgentConfigurationUpsert, ApplicationRegistrationUpsert, PermissionGrantUpsert,
    ProtocolErrorBody, ProtocolErrorCode, ProtocolResponse, ResourceLabelUpsert, SecurityOverview,
};

use crate::service::{HalquenService, internal_error, now_ms};

const IMMUTABLE_RULE_IDS: [&str; 4] = [
    "immutable.secret-to-external",
    "immutable.production-destructive",
    "immutable.system-critical",
    "immutable.untrusted-authority-mutation",
];

impl<E: Executor> HalquenService<E> {
    pub(crate) fn security_overview(&self) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let now = now_ms();
        let permissions = self
            .database
            .active_permission_grants(now, 200)
            .map_err(internal_error)?;
        let labels = self
            .database
            .list_resource_labels(200)
            .map_err(internal_error)?;
        let agents = self.database.list_agents(100).map_err(internal_error)?;
        let sessions = self
            .database
            .list_agent_sessions(200)
            .map_err(internal_error)?;
        let applications = self
            .database
            .list_registered_applications(200)
            .map_err(internal_error)?;
        Ok(ProtocolResponse::SecurityOverview {
            overview: SecurityOverview {
                profile: self.database.security_profile().map_err(internal_error)?,
                immutable_rule_ids: IMMUTABLE_RULE_IDS
                    .iter()
                    .map(|rule| (*rule).to_owned())
                    .collect(),
                active_permissions: u32::try_from(permissions.len()).unwrap_or(u32::MAX),
                resource_labels: u32::try_from(labels.len()).unwrap_or(u32::MAX),
                configured_agents: u32::try_from(agents.len()).unwrap_or(u32::MAX),
                active_agent_sessions: u32::try_from(
                    sessions
                        .iter()
                        .filter(|session| session.status == AgentSessionStatus::Running)
                        .count(),
                )
                .unwrap_or(u32::MAX),
                registered_applications: u32::try_from(applications.len()).unwrap_or(u32::MAX),
            },
        })
    }

    pub(crate) fn update_security_profile(
        &mut self,
        profile: SecurityProfile,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        self.database
            .update_security_profile(profile, now_ms())
            .map_err(internal_error)?;
        Ok(ProtocolResponse::SecurityProfileUpdated { profile })
    }

    pub(crate) fn list_permission_grants(
        &self,
        limit: u16,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        if limit == 0 || limit > 200 {
            return Err(validation("permission limit must be between 1 and 200"));
        }
        Ok(ProtocolResponse::PermissionGrants {
            grants: self
                .database
                .list_permission_grants(limit)
                .map_err(internal_error)?,
        })
    }

    pub(crate) fn upsert_permission_grant(
        &mut self,
        input: PermissionGrantUpsert,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let descriptor = self
            .registry
            .get(&input.capability_id)
            .ok_or_else(|| validation("permission capability is not registered"))?;
        if input.arguments.kind() != descriptor.arguments || input.resources.len() > 16 {
            return Err(validation("permission scope does not match the capability"));
        }
        if let Some(agent_id) = &input.agent_id {
            let exists = self
                .database
                .list_agents(100)
                .map_err(internal_error)?
                .iter()
                .any(|agent| &agent.id == agent_id);
            if !exists {
                return Err(validation("permission agent is not configured"));
            }
        }
        let now = now_ms();
        if input.expires_at_ms.is_some_and(|expiry| expiry <= now) {
            return Err(validation("permission expiration must be in the future"));
        }
        let (session, expires_at_ms, use_limit) = match input.lifetime {
            PermissionLifetime::Once => (None, None, Some(1)),
            PermissionLifetime::Session => {
                let session = input
                    .session
                    .ok_or_else(|| validation("session permission requires a session"))?;
                match &session {
                    PermissionSessionScope::Agent(session_id) => {
                        let agent_id = input.agent_id.as_ref().ok_or_else(|| {
                            validation("agent-session permission requires an agent")
                        })?;
                        let valid = self
                            .database
                            .list_agent_sessions(200)
                            .map_err(internal_error)?
                            .into_iter()
                            .any(|candidate| {
                                &candidate.id == session_id
                                    && &candidate.agent_id == agent_id
                                    && candidate.status == AgentSessionStatus::Running
                            });
                        if !valid {
                            return Err(validation("agent session is not active for this agent"));
                        }
                    }
                    PermissionSessionScope::Daemon(id) if id != &self.daemon_session_id => {
                        return Err(validation("daemon session is not current"));
                    }
                    PermissionSessionScope::Chat(_) | PermissionSessionScope::Daemon(_) => {}
                }
                (Some(session), None, None)
            }
            PermissionLifetime::Until => {
                let expiry = input
                    .expires_at_ms
                    .ok_or_else(|| validation("until permission requires an expiration"))?;
                (None, Some(expiry), None)
            }
            PermissionLifetime::Always => (None, None, None),
        };
        let grant = PermissionGrant {
            id: input.id.unwrap_or_else(PermissionId::generate),
            effect: input.effect,
            lifetime: input.lifetime,
            scope: PermissionScope {
                capability_id: input.capability_id,
                arguments: input.arguments,
                resources: input.resources,
                destination: input.destination,
            },
            session,
            agent_id: input.agent_id,
            granted_by: ActionOrigin::UserExplicit,
            granted_at_ms: now,
            expires_at_ms,
            revoked_at_ms: None,
            use_limit,
            use_count: 0,
        };
        self.database
            .upsert_permission_grant(&grant)
            .map_err(internal_error)?;
        Ok(ProtocolResponse::PermissionSaved { grant })
    }

    pub(crate) fn revoke_permission_grant(
        &mut self,
        id: &PermissionId,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        Ok(ProtocolResponse::PermissionRevoked {
            revoked: self
                .database
                .revoke_permission(id, now_ms())
                .map_err(internal_error)?,
        })
    }

    pub(crate) fn list_resource_labels(
        &self,
        limit: u16,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        if limit == 0 || limit > 200 {
            return Err(validation("resource-label limit must be between 1 and 200"));
        }
        Ok(ProtocolResponse::ResourceLabels {
            labels: self
                .database
                .list_resource_labels(limit)
                .map_err(internal_error)?,
        })
    }

    pub(crate) fn upsert_resource_label(
        &mut self,
        input: ResourceLabelUpsert,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let now = now_ms();
        let label = ResourceLabel {
            id: input.id.unwrap_or_else(ResourceLabelId::generate),
            name: input.name,
            resource_kind: input.resource_kind,
            match_kind: input.match_kind,
            pattern: input.pattern,
            classification: input.classification,
            data_classification: input.data_classification,
            created_at_ms: now,
            updated_at_ms: now,
        };
        self.database.upsert_resource_label(&label).map_err(|_| {
            validation("resource label is invalid or conflicts with an exact label")
        })?;
        Ok(ProtocolResponse::ResourceLabelSaved { label })
    }

    pub(crate) fn remove_resource_label(
        &mut self,
        id: &ResourceLabelId,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        Ok(ProtocolResponse::ResourceLabelRemoved {
            removed: self
                .database
                .remove_resource_label(id)
                .map_err(internal_error)?,
        })
    }

    pub(crate) fn list_agents(&self, limit: u16) -> Result<ProtocolResponse, ProtocolErrorBody> {
        if limit == 0 || limit > 100 {
            return Err(validation("agent limit must be between 1 and 100"));
        }
        Ok(ProtocolResponse::Agents {
            agents: self.database.list_agents(limit).map_err(internal_error)?,
        })
    }

    pub(crate) fn upsert_agent(
        &mut self,
        input: AgentConfigurationUpsert,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let identity = if input.transport == halquen_domain::AgentTransport::Cli {
            Some(
                inspect_executable(
                    &input.executable,
                    input.ownership,
                    input.sha256_hex.as_deref(),
                )
                .map_err(|_| validation("agent executable identity is not trusted"))?,
            )
        } else {
            None
        };
        let agent = input.into_configuration(now_ms(), identity);
        self.database
            .upsert_agent(&agent)
            .map_err(|_| validation("agent configuration is invalid"))?;
        Ok(ProtocolResponse::AgentSaved { agent })
    }

    pub(crate) fn remove_agent(
        &mut self,
        id: &AgentId,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        Ok(ProtocolResponse::AgentRemoved {
            removed: self.database.remove_agent(id).map_err(internal_error)?,
        })
    }

    pub(crate) fn list_agent_sessions(
        &self,
        limit: u16,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        if limit == 0 || limit > 200 {
            return Err(validation("agent-session limit must be between 1 and 200"));
        }
        Ok(ProtocolResponse::AgentSessions {
            sessions: self
                .database
                .list_agent_sessions(limit)
                .map_err(internal_error)?,
        })
    }

    pub(crate) fn list_registered_applications(
        &self,
        limit: u16,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        if limit == 0 || limit > 200 {
            return Err(validation("application limit must be between 1 and 200"));
        }
        Ok(ProtocolResponse::RegisteredApplications {
            applications: self
                .database
                .list_registered_applications(limit)
                .map_err(internal_error)?,
        })
    }

    pub(crate) fn upsert_registered_application(
        &mut self,
        input: ApplicationRegistrationUpsert,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let identity = inspect_executable(
            &input.executable,
            input.ownership,
            input.sha256_hex.as_deref(),
        )
        .map_err(|_| validation("application executable identity is not trusted"))?;
        let now = now_ms();
        let application = RegisteredApplication {
            entity_id: input.entity_id,
            display_name: input.display_name,
            executable: identity.canonical_path.clone(),
            arguments: input.arguments,
            ownership: input.ownership,
            identity,
            enabled: input.enabled,
            created_at_ms: now,
            updated_at_ms: now,
        };
        application
            .validate()
            .map_err(|_| validation("application registration is invalid"))?;
        let mut registry = self
            .applications
            .write()
            .map_err(|_| internal_error("application registry lock failed"))?;
        self.database
            .upsert_registered_application(&application)
            .map_err(internal_error)?;
        registry
            .upsert(application.clone())
            .map_err(internal_error)?;
        Ok(ProtocolResponse::RegisteredApplicationSaved { application })
    }

    pub(crate) fn remove_registered_application(
        &mut self,
        entity_id: &EntityId,
    ) -> Result<ProtocolResponse, ProtocolErrorBody> {
        let mut registry = self
            .applications
            .write()
            .map_err(|_| internal_error("application registry lock failed"))?;
        let removed = self
            .database
            .remove_registered_application(entity_id)
            .map_err(internal_error)?;
        if removed {
            registry.remove(entity_id);
        }
        Ok(ProtocolResponse::RegisteredApplicationRemoved { removed })
    }
}

fn validation(message: &str) -> ProtocolErrorBody {
    ProtocolErrorBody {
        code: ProtocolErrorCode::Validation,
        message: message.to_owned(),
    }
}
