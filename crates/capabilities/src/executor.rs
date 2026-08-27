use std::process::Stdio;

use halquen_domain::ActionArguments;
use halquen_policy::ExecutionAuthorization;
use thiserror::Error;
use tokio::process::Command;

use crate::{SharedApplicationRegistry, verify_executable};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionOutcome {
    pub code: ExecutionResultCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionResultCode {
    Simulated,
    Launched,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExecutionError {
    #[error("policy produced an internally inconsistent authorization")]
    InvalidAuthorization,
    #[error("application is not present in the trusted registry")]
    UnknownApplication,
    #[error("registered application executable identity is no longer valid")]
    ExecutableIdentityChanged,
    #[error("registered application could not be launched")]
    SpawnFailed,
    #[error("application registry is unavailable")]
    RegistryUnavailable,
}

#[allow(async_fn_in_trait)]
pub trait Executor {
    async fn execute(
        &self,
        authorization: ExecutionAuthorization,
    ) -> Result<ExecutionOutcome, ExecutionError>;
}

#[derive(Debug, Clone, Copy)]
pub struct DryRunExecutor;

impl DryRunExecutor {
    pub fn new() -> Self {
        Self
    }
}

impl Executor for DryRunExecutor {
    async fn execute(
        &self,
        authorization: ExecutionAuthorization,
    ) -> Result<ExecutionOutcome, ExecutionError> {
        validate_authorization(&authorization)?;
        Ok(ExecutionOutcome {
            code: ExecutionResultCode::Simulated,
        })
    }
}

impl Default for DryRunExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct RealLinuxExecutor {
    applications: SharedApplicationRegistry,
}

impl RealLinuxExecutor {
    pub fn new(applications: SharedApplicationRegistry) -> Self {
        Self { applications }
    }
}

impl Executor for RealLinuxExecutor {
    async fn execute(
        &self,
        authorization: ExecutionAuthorization,
    ) -> Result<ExecutionOutcome, ExecutionError> {
        validate_authorization(&authorization)?;
        let ActionArguments::OpenApp { app } = &authorization.action().arguments else {
            return Err(ExecutionError::InvalidAuthorization);
        };
        let application = self
            .applications
            .read()
            .map_err(|_| ExecutionError::RegistryUnavailable)?
            .get(app)
            .filter(|application| application.enabled)
            .cloned()
            .ok_or(ExecutionError::UnknownApplication)?;
        let executable = verify_executable(&application.identity, application.ownership)
            .map_err(|_| ExecutionError::ExecutableIdentityChanged)?;

        let mut command = Command::new(executable);
        command
            .args(&application.arguments)
            .env_clear()
            .env("PATH", "/usr/local/bin:/usr/bin:/bin")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for variable in [
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "XAUTHORITY",
            "DBUS_SESSION_BUS_ADDRESS",
            "XDG_RUNTIME_DIR",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
            "HOME",
            "LANG",
            "LC_ALL",
        ] {
            if let Some(value) = std::env::var_os(variable) {
                command.env(variable, value);
            }
        }
        let mut child = command.spawn().map_err(|_| ExecutionError::SpawnFailed)?;
        tokio::spawn(async move {
            let _ = child.wait().await;
        });
        Ok(ExecutionOutcome {
            code: ExecutionResultCode::Launched,
        })
    }
}

#[derive(Debug, Clone)]
pub enum RuntimeExecutor {
    DryRun(DryRunExecutor),
    RealLinux(RealLinuxExecutor),
}

impl Executor for RuntimeExecutor {
    async fn execute(
        &self,
        authorization: ExecutionAuthorization,
    ) -> Result<ExecutionOutcome, ExecutionError> {
        match self {
            Self::DryRun(executor) => executor.execute(authorization).await,
            Self::RealLinux(executor) => executor.execute(authorization).await,
        }
    }
}

fn validate_authorization(authorization: &ExecutionAuthorization) -> Result<(), ExecutionError> {
    let descriptor = authorization.descriptor();
    let request = authorization.action();
    if descriptor.validate().is_err()
        || request.capability_id != descriptor.id
        || request.arguments.kind() != descriptor.arguments
    {
        return Err(ExecutionError::InvalidAuthorization);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, RwLock};

    use halquen_domain::{
        ActionArguments, ActionRequest, EntityId, ExecutableOwnership, RegisteredApplication,
    };
    use halquen_policy::PolicyEngine;

    use crate::open_app_descriptor;

    use super::*;

    #[tokio::test]
    async fn executes_only_with_matching_policy_authorization() {
        let descriptor = open_app_descriptor();
        let request = ActionRequest::new(
            descriptor.id.clone(),
            ActionArguments::OpenApp {
                app: EntityId::new("app:telegram").unwrap(),
            },
        );
        let evaluation = PolicyEngine::new().authorize(
            &descriptor,
            request,
            halquen_domain::ExecutionId::generate(),
        );
        let outcome = DryRunExecutor::new()
            .execute(evaluation.into_authorization().unwrap())
            .await
            .unwrap();
        assert_eq!(outcome.code, ExecutionResultCode::Simulated);
    }

    #[test]
    fn policy_rejects_argument_mismatch_before_authorization() {
        let descriptor = open_app_descriptor();
        let request = ActionRequest::new(descriptor.id.clone(), ActionArguments::None);
        let result = PolicyEngine::new().authorize(
            &descriptor,
            request,
            halquen_domain::ExecutionId::generate(),
        );
        assert!(result.authorization().is_none());
    }

    #[tokio::test]
    async fn real_executor_launches_only_a_registered_identity_without_a_shell() {
        let Some(executable) = ["/usr/bin/true", "/usr/bin/printf"]
            .into_iter()
            .find(|candidate| std::path::Path::new(candidate).is_file())
        else {
            return;
        };
        let identity =
            crate::inspect_executable(executable, ExecutableOwnership::RootOnly, None).unwrap();
        let app = EntityId::new("app:non_gui_test_fixture").unwrap();
        let application = RegisteredApplication {
            entity_id: app.clone(),
            display_name: "Non-GUI test fixture".to_owned(),
            executable: executable.to_owned(),
            arguments: Vec::new(),
            ownership: ExecutableOwnership::RootOnly,
            identity,
            enabled: true,
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        let mut applications = crate::ApplicationRegistry::new();
        applications.upsert(application).unwrap();
        let executor = RealLinuxExecutor::new(Arc::new(RwLock::new(applications)));
        let descriptor = open_app_descriptor();
        let action = ActionRequest::new(descriptor.id.clone(), ActionArguments::OpenApp { app });
        let authorization = PolicyEngine::new()
            .authorize(&descriptor, action, halquen_domain::ExecutionId::generate())
            .into_authorization()
            .unwrap();
        assert_eq!(
            executor.execute(authorization).await.unwrap().code,
            ExecutionResultCode::Launched
        );
    }
}
