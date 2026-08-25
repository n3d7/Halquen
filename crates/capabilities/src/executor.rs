use halquen_policy::ExecutionAuthorization;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionOutcome {
    pub code: ExecutionResultCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionResultCode {
    Simulated,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExecutionError {
    #[error("policy produced an internally inconsistent authorization")]
    InvalidAuthorization,
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
        let descriptor = authorization.descriptor();
        let request = authorization.action();
        if descriptor.validate().is_err()
            || request.capability_id != descriptor.id
            || request.arguments.kind() != descriptor.arguments
        {
            return Err(ExecutionError::InvalidAuthorization);
        }
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

#[cfg(test)]
mod tests {
    use halquen_domain::{ActionArguments, ActionRequest, EntityId};
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
}
