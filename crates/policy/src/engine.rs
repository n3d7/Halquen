use std::collections::BTreeSet;

use halquen_domain::{
    ActionRequest, CapabilityDescriptor, ConfirmationPolicy, ExecutionId, RiskClass,
    ScopeRequirement,
};

use crate::{
    ExecutionAuthorization, PolicyDecision, PolicyEvaluation, PolicyOutcome, PolicyReason,
};

#[derive(Debug, Clone, Default)]
pub struct PolicyContext {
    granted_scopes: BTreeSet<ScopeRequirement>,
}

impl PolicyContext {
    pub fn grant(&mut self, scope: ScopeRequirement) {
        self.granted_scopes.insert(scope);
    }

    fn missing_scope(&self, descriptor: &CapabilityDescriptor) -> Option<ScopeRequirement> {
        descriptor
            .scope_requirements
            .iter()
            .find(|scope| !self.granted_scopes.contains(*scope))
            .cloned()
    }

    fn normalized_grants(&self) -> Vec<ScopeRequirement> {
        self.granted_scopes.iter().cloned().collect()
    }
}

#[derive(Debug, Clone)]
pub struct PolicyEngine {
    allow_reversible_local_writes: bool,
}

impl PolicyEngine {
    pub fn new() -> Self {
        Self {
            allow_reversible_local_writes: true,
        }
    }

    pub fn with_reversible_local_writes(mut self, allow: bool) -> Self {
        self.allow_reversible_local_writes = allow;
        self
    }

    pub fn evaluate(&self, capability: &CapabilityDescriptor) -> PolicyDecision {
        self.evaluate_with_context(capability, &PolicyContext::default())
    }

    pub fn evaluate_with_context(
        &self,
        capability: &CapabilityDescriptor,
        context: &PolicyContext,
    ) -> PolicyDecision {
        self.decide(capability, context)
    }

    pub fn authorize(
        &self,
        capability: &CapabilityDescriptor,
        action: ActionRequest,
        execution_id: ExecutionId,
    ) -> PolicyEvaluation {
        self.authorize_with_context(capability, action, execution_id, &PolicyContext::default())
    }

    pub fn authorize_with_context(
        &self,
        capability: &CapabilityDescriptor,
        action: ActionRequest,
        execution_id: ExecutionId,
        context: &PolicyContext,
    ) -> PolicyEvaluation {
        if action.capability_id != capability.id || action.arguments.kind() != capability.arguments
        {
            return blocked(PolicyOutcome::Deny, PolicyReason::InvalidActionContract);
        }

        let decision = self.decide(capability, context);
        if decision.is_allowed() {
            let authorization = ExecutionAuthorization::new(
                execution_id,
                capability.clone(),
                action,
                context.normalized_grants(),
            );
            PolicyEvaluation::allowed(decision, authorization)
        } else {
            PolicyEvaluation::blocked(decision)
        }
    }

    pub fn authorize_confirmed_once(
        &self,
        capability: &CapabilityDescriptor,
        action: ActionRequest,
        execution_id: ExecutionId,
        context: &PolicyContext,
    ) -> PolicyEvaluation {
        if action.capability_id != capability.id || action.arguments.kind() != capability.arguments
        {
            return blocked(PolicyOutcome::Deny, PolicyReason::InvalidActionContract);
        }
        let initial = self.decide(capability, context);
        if initial.outcome == PolicyOutcome::Deny {
            return PolicyEvaluation::blocked(initial);
        }
        let decision = if initial.outcome == PolicyOutcome::Confirm {
            PolicyDecision {
                outcome: PolicyOutcome::Allow,
                reason: PolicyReason::UserConfirmedOnce,
            }
        } else {
            initial
        };
        let authorization = ExecutionAuthorization::new(
            execution_id,
            capability.clone(),
            action,
            context.normalized_grants(),
        );
        PolicyEvaluation::allowed(decision, authorization)
    }

    fn decide(&self, capability: &CapabilityDescriptor, context: &PolicyContext) -> PolicyDecision {
        if capability.validate().is_err() {
            return decision(PolicyOutcome::Deny, PolicyReason::InvalidDescriptor);
        }

        if let Some(scope) = context.missing_scope(capability) {
            return decision(PolicyOutcome::Deny, PolicyReason::MissingScope { scope });
        }

        match capability.risk {
            RiskClass::Privileged => PolicyDecision {
                outcome: PolicyOutcome::Deny,
                reason: PolicyReason::PrivilegedDenied,
            },
            RiskClass::Unknown => PolicyDecision {
                outcome: PolicyOutcome::Deny,
                reason: PolicyReason::UnknownRiskDenied,
            },
            _ if capability.confirmation == ConfirmationPolicy::Always => PolicyDecision {
                outcome: PolicyOutcome::Confirm,
                reason: PolicyReason::CapabilityRequiresConfirmation,
            },
            RiskClass::ReadOnly => PolicyDecision {
                outcome: PolicyOutcome::Allow,
                reason: PolicyReason::ReadOnlyBaseline,
            },
            RiskClass::LocalSideEffect => PolicyDecision {
                outcome: PolicyOutcome::Allow,
                reason: PolicyReason::LocalSideEffectBaseline,
            },
            RiskClass::ReversibleLocalWrite if self.allow_reversible_local_writes => {
                PolicyDecision {
                    outcome: PolicyOutcome::Allow,
                    reason: PolicyReason::ReversibleLocalWriteBaseline,
                }
            }
            RiskClass::ReversibleLocalWrite => PolicyDecision {
                outcome: PolicyOutcome::Confirm,
                reason: PolicyReason::ReversibleLocalWriteRequiresConfirmation,
            },
            RiskClass::ExternalSideEffect => PolicyDecision {
                outcome: PolicyOutcome::Confirm,
                reason: PolicyReason::ExternalSideEffectRequiresConfirmation,
            },
            RiskClass::Destructive => PolicyDecision {
                outcome: PolicyOutcome::Confirm,
                reason: PolicyReason::DestructiveRequiresConfirmation,
            },
        }
    }
}

fn blocked(outcome: PolicyOutcome, reason: PolicyReason) -> PolicyEvaluation {
    PolicyEvaluation::blocked(PolicyDecision { outcome, reason })
}

fn decision(outcome: PolicyOutcome, reason: PolicyReason) -> PolicyDecision {
    PolicyDecision { outcome, reason }
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use halquen_domain::{
        ActionArgumentKind, ActionArguments, CapabilityId, ConfirmationPolicy, EntityId,
    };

    use super::*;

    fn descriptor(risk: RiskClass) -> CapabilityDescriptor {
        CapabilityDescriptor {
            id: CapabilityId::new("test.operation").unwrap(),
            version: 1,
            description: "Test operation".to_owned(),
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

    #[test]
    fn read_only_is_allowed_and_authorized() {
        let descriptor = descriptor(RiskClass::ReadOnly);
        let action = ActionRequest::new(descriptor.id.clone(), ActionArguments::None);
        let result = PolicyEngine::new().authorize(&descriptor, action, ExecutionId::generate());
        assert_eq!(result.decision.outcome, PolicyOutcome::Allow);
        assert!(result.authorization().is_some());
    }

    #[test]
    fn external_and_destructive_actions_require_confirmation() {
        for risk in [RiskClass::ExternalSideEffect, RiskClass::Destructive] {
            let descriptor = descriptor(risk);
            let action = ActionRequest::new(descriptor.id.clone(), ActionArguments::None);
            let result =
                PolicyEngine::new().authorize(&descriptor, action, ExecutionId::generate());
            assert_eq!(result.decision.outcome, PolicyOutcome::Confirm);
            assert!(result.authorization().is_none());
        }
    }

    #[test]
    fn privileged_and_unknown_risk_fail_closed() {
        for risk in [RiskClass::Privileged, RiskClass::Unknown] {
            let descriptor = descriptor(risk);
            let action = ActionRequest::new(descriptor.id.clone(), ActionArguments::None);
            let result =
                PolicyEngine::new().authorize(&descriptor, action, ExecutionId::generate());
            assert_eq!(result.decision.outcome, PolicyOutcome::Deny);
            assert!(result.authorization().is_none());
        }
    }

    #[test]
    fn missing_scope_is_denied() {
        let mut capability = descriptor(RiskClass::ReadOnly);
        capability.scope_requirements.push(ScopeRequirement::Named {
            scope: "calendar.read".to_owned(),
        });
        let result = PolicyEngine::new().evaluate(&capability);
        assert!(matches!(result.reason, PolicyReason::MissingScope { .. }));
    }

    #[test]
    fn authorization_captures_normalized_scope_context() {
        let mut capability = descriptor(RiskClass::ReadOnly);
        let required = ScopeRequirement::Named {
            scope: "calendar.read".to_owned(),
        };
        capability.scope_requirements.push(required.clone());
        let mut context = PolicyContext::default();
        context.grant(ScopeRequirement::Named {
            scope: "z.extra".to_owned(),
        });
        context.grant(required.clone());
        let action = ActionRequest::new(capability.id.clone(), ActionArguments::None);
        let evaluation = PolicyEngine::new().authorize_with_context(
            &capability,
            action,
            ExecutionId::generate(),
            &context,
        );
        assert_eq!(
            evaluation.authorization().unwrap().granted_scopes(),
            &[
                required,
                ScopeRequirement::Named {
                    scope: "z.extra".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn authorization_is_bound_to_exact_action_and_version() {
        let mut v1 = descriptor(RiskClass::ReadOnly);
        v1.id = CapabilityId::new("system.open_app").unwrap();
        v1.arguments = ActionArgumentKind::OpenApp;
        let telegram = ActionRequest::new(
            v1.id.clone(),
            ActionArguments::OpenApp {
                app: EntityId::new("app:telegram").unwrap(),
            },
        );
        let discord = ActionRequest::new(
            v1.id.clone(),
            ActionArguments::OpenApp {
                app: EntityId::new("app:discord").unwrap(),
            },
        );
        let evaluation =
            PolicyEngine::new().authorize(&v1, telegram.clone(), ExecutionId::generate());
        let authorization = evaluation.authorization().unwrap();
        assert!(authorization.matches(&v1, &telegram));
        assert!(!authorization.matches(&v1, &discord));

        let mut v2 = v1.clone();
        v2.version = 2;
        assert!(!authorization.matches(&v2, &telegram));
    }

    #[test]
    fn explicit_confirmation_authorizes_once_but_never_overrides_deny() {
        let external = descriptor(RiskClass::ExternalSideEffect);
        let action = ActionRequest::new(external.id.clone(), ActionArguments::None);
        let result = PolicyEngine::new().authorize_confirmed_once(
            &external,
            action,
            ExecutionId::generate(),
            &PolicyContext::default(),
        );
        assert_eq!(result.decision.reason, PolicyReason::UserConfirmedOnce);
        assert!(result.authorization().is_some());

        let privileged = descriptor(RiskClass::Privileged);
        let action = ActionRequest::new(privileged.id.clone(), ActionArguments::None);
        let result = PolicyEngine::new().authorize_confirmed_once(
            &privileged,
            action,
            ExecutionId::generate(),
            &PolicyContext::default(),
        );
        assert_eq!(result.decision.outcome, PolicyOutcome::Deny);
        assert!(result.authorization().is_none());
    }
}
