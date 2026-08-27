use std::collections::BTreeSet;

use halquen_domain::{
    ActionContext, ActionOrigin, ActionProposal, ActionRequest, CapabilityDescriptor,
    ConfirmationPolicy, DataClassification, DestinationClass, ExecutionId, PermissionEffect,
    PermissionGrant, PermissionSessionScope, ResourceClassification, RiskClass, ScopeRequirement,
    SecurityProfile,
};

use crate::{
    ConfirmationLevel, ExecutionAuthorization, PolicyDecision, PolicyEvaluation, PolicyOutcome,
    PolicyReason,
};

#[derive(Debug, Clone)]
pub struct PolicyContext {
    granted_scopes: BTreeSet<ScopeRequirement>,
    action_context: Option<ActionContext>,
    permission_grants: Vec<PermissionGrant>,
    profile: SecurityProfile,
    now_ms: i64,
    session: Option<PermissionSessionScope>,
    agent_id: Option<halquen_domain::AgentId>,
}

impl PolicyContext {
    pub fn grant(&mut self, scope: ScopeRequirement) {
        self.granted_scopes.insert(scope);
    }

    pub fn set_action_context(&mut self, context: ActionContext) {
        if let Some(identity) = &context.agent {
            self.session = Some(PermissionSessionScope::Agent(identity.session_id.clone()));
            self.agent_id = Some(identity.agent_id.clone());
        }
        self.action_context = Some(context);
    }

    pub fn add_permission_grant(&mut self, grant: PermissionGrant) {
        self.permission_grants.push(grant);
    }

    pub fn set_profile(&mut self, profile: SecurityProfile) {
        self.profile = profile;
    }

    pub fn set_now_ms(&mut self, now_ms: i64) {
        self.now_ms = now_ms;
    }

    pub fn set_session_id(&mut self, session_id: Option<halquen_domain::ChatSessionId>) {
        if self.agent_id.is_none() {
            self.session = session_id.map(PermissionSessionScope::Chat);
        }
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

    fn context_for_action(&self) -> ActionContext {
        self.action_context
            .clone()
            .unwrap_or_else(|| ActionContext::trusted_user(None))
    }

    fn matching_permission<'a>(
        &'a self,
        proposal: &ActionProposal,
        effect: PermissionEffect,
    ) -> Option<&'a PermissionGrant> {
        self.permission_grants.iter().find(|grant| {
            grant.effect == effect
                && grant.is_active(self.now_ms, self.session.as_ref(), self.agent_id.as_ref())
                && grant.scope.matches(proposal)
        })
    }
}

impl Default for PolicyContext {
    fn default() -> Self {
        Self {
            granted_scopes: BTreeSet::new(),
            action_context: None,
            permission_grants: Vec::new(),
            profile: SecurityProfile::Balanced,
            now_ms: 0,
            session: None,
            agent_id: None,
        }
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
        let proposal = ActionProposal {
            action: ActionRequest::new(capability.id.clone(), default_arguments(capability)),
            context: context.context_for_action(),
        };
        self.decide(capability, &proposal, context)
    }

    pub fn evaluate_proposal(
        &self,
        capability: &CapabilityDescriptor,
        proposal: &ActionProposal,
        context: &PolicyContext,
    ) -> PolicyDecision {
        self.decide(capability, proposal, context)
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
        let proposal = ActionProposal {
            action,
            context: context.context_for_action(),
        };
        self.authorize_proposal(capability, proposal, execution_id, context)
    }

    pub fn authorize_proposal(
        &self,
        capability: &CapabilityDescriptor,
        proposal: ActionProposal,
        execution_id: ExecutionId,
        context: &PolicyContext,
    ) -> PolicyEvaluation {
        if proposal.action.capability_id != capability.id
            || proposal.action.arguments.kind() != capability.arguments
        {
            return blocked(
                PolicyReason::InvalidActionContract,
                "core.invalid-action-contract",
                true,
            );
        }
        let decision = self.decide(capability, &proposal, context);
        if decision.is_allowed() {
            PolicyEvaluation::allowed(
                decision,
                ExecutionAuthorization::new(
                    execution_id,
                    capability.clone(),
                    proposal.action,
                    context.normalized_grants(),
                ),
            )
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
        let proposal = ActionProposal {
            action,
            context: context.context_for_action(),
        };
        self.authorize_confirmed_proposal_once(capability, proposal, execution_id, context)
    }

    pub fn authorize_confirmed_proposal_once(
        &self,
        capability: &CapabilityDescriptor,
        proposal: ActionProposal,
        execution_id: ExecutionId,
        context: &PolicyContext,
    ) -> PolicyEvaluation {
        if proposal.action.capability_id != capability.id
            || proposal.action.arguments.kind() != capability.arguments
        {
            return blocked(
                PolicyReason::InvalidActionContract,
                "core.invalid-action-contract",
                true,
            );
        }
        let initial = self.decide(capability, &proposal, context);
        if initial.outcome == PolicyOutcome::Deny {
            return PolicyEvaluation::blocked(initial);
        }
        let decision = if initial.outcome == PolicyOutcome::Confirm {
            decision(
                PolicyOutcome::Allow,
                PolicyReason::UserConfirmedOnce,
                "user.confirmed-once",
                false,
                None,
                None,
            )
        } else {
            initial
        };
        PolicyEvaluation::allowed(
            decision,
            ExecutionAuthorization::new(
                execution_id,
                capability.clone(),
                proposal.action,
                context.normalized_grants(),
            ),
        )
    }

    fn decide(
        &self,
        capability: &CapabilityDescriptor,
        proposal: &ActionProposal,
        context: &PolicyContext,
    ) -> PolicyDecision {
        if capability.validate().is_err() {
            return decision(
                PolicyOutcome::Deny,
                PolicyReason::InvalidDescriptor,
                "core.invalid-descriptor",
                true,
                None,
                None,
            );
        }
        if proposal.context.validate().is_err() {
            return decision(
                PolicyOutcome::Deny,
                PolicyReason::InvalidProvenance,
                "core.invalid-provenance",
                true,
                None,
                None,
            );
        }
        if let Some(scope) = context.missing_scope(capability) {
            return decision(
                PolicyOutcome::Deny,
                PolicyReason::MissingScope { scope },
                "core.missing-scope",
                false,
                None,
                None,
            );
        }
        if secret_to_external(proposal) {
            return decision(
                PolicyOutcome::Deny,
                PolicyReason::ImmutableSecretExfiltrationDeny,
                "immutable.secret-to-external",
                true,
                None,
                None,
            );
        }
        if capability.risk == RiskClass::Destructive
            && (has_resource(proposal, ResourceClassification::Production)
                || proposal
                    .context
                    .data_flow
                    .as_ref()
                    .is_some_and(|flow| flow.data_classification == DataClassification::Production))
        {
            return decision(
                PolicyOutcome::Deny,
                PolicyReason::ImmutableProductionDestructiveDeny,
                "immutable.production-destructive",
                true,
                None,
                None,
            );
        }
        if has_resource(proposal, ResourceClassification::SystemCritical)
            && matches!(
                capability.risk,
                RiskClass::Destructive | RiskClass::Privileged | RiskClass::ExternalSideEffect
            )
        {
            return decision(
                PolicyOutcome::Deny,
                PolicyReason::ImmutableSystemCriticalDeny,
                "immutable.system-critical",
                true,
                None,
                None,
            );
        }
        if matches!(
            proposal.context.origin,
            ActionOrigin::AiProposal
                | ActionOrigin::ExternalContent
                | ActionOrigin::Plugin
                | ActionOrigin::Agent
        ) && matches!(
            capability.id.as_str().split('.').next(),
            Some("security" | "permission" | "policy")
        ) {
            return decision(
                PolicyOutcome::Deny,
                PolicyReason::UntrustedAuthorityMutationDenied,
                "immutable.untrusted-authority-mutation",
                true,
                None,
                None,
            );
        }
        if let Some(grant) = context.matching_permission(proposal, PermissionEffect::Deny) {
            return decision(
                PolicyOutcome::Deny,
                PolicyReason::PersistentExactDeny,
                "grant.exact-deny",
                false,
                None,
                grant.expires_at_ms,
            );
        }
        if matches!(capability.risk, RiskClass::Privileged | RiskClass::Unknown) {
            return if capability.risk == RiskClass::Privileged {
                decision(
                    PolicyOutcome::Deny,
                    PolicyReason::PrivilegedDenied,
                    "baseline.privileged-deny",
                    false,
                    None,
                    None,
                )
            } else {
                decision(
                    PolicyOutcome::Deny,
                    PolicyReason::UnknownRiskDenied,
                    "baseline.unknown-risk-deny",
                    false,
                    None,
                    None,
                )
            };
        }
        let baseline = self.baseline(capability, proposal, context.profile);
        if baseline.outcome == PolicyOutcome::Confirm
            && let Some(grant) = context.matching_permission(proposal, PermissionEffect::Allow)
        {
            return decision(
                PolicyOutcome::Allow,
                PolicyReason::PersistentExactAllow,
                "grant.exact-allow",
                false,
                None,
                grant.expires_at_ms,
            );
        }
        baseline
    }

    fn baseline(
        &self,
        capability: &CapabilityDescriptor,
        proposal: &ActionProposal,
        profile: SecurityProfile,
    ) -> PolicyDecision {
        if has_resource(proposal, ResourceClassification::Production) {
            return confirmation(
                PolicyReason::ProductionResourceRequiresConfirmation,
                "resource.production-confirm",
                ConfirmationLevel::Sensitive,
            );
        }
        if proposal.context.resources.iter().any(|resource| {
            matches!(
                resource.classification,
                ResourceClassification::Sensitive | ResourceClassification::Secret
            )
        }) {
            return confirmation(
                PolicyReason::SensitiveResourceRequiresConfirmation,
                "resource.sensitive-confirm",
                ConfirmationLevel::Sensitive,
            );
        }
        if proposal.context.origin == ActionOrigin::Agent && capability.side_effect {
            return confirmation(
                PolicyReason::AgentSideEffectRequiresConfirmation,
                "agent.side-effect-confirm",
                ConfirmationLevel::Standard,
            );
        }
        if profile == SecurityProfile::Strict && capability.side_effect {
            return confirmation(
                PolicyReason::StrictProfileRequiresConfirmation,
                "profile.strict-side-effect-confirm",
                ConfirmationLevel::Standard,
            );
        }
        if capability.confirmation == ConfirmationPolicy::Always {
            return confirmation(
                PolicyReason::CapabilityRequiresConfirmation,
                "capability.always-confirm",
                ConfirmationLevel::Standard,
            );
        }
        match capability.risk {
            RiskClass::ReadOnly => allow(PolicyReason::ReadOnlyBaseline, "baseline.read-only"),
            RiskClass::LocalSideEffect => allow(
                PolicyReason::LocalSideEffectBaseline,
                "baseline.local-side-effect",
            ),
            RiskClass::ReversibleLocalWrite if self.allow_reversible_local_writes => allow(
                PolicyReason::ReversibleLocalWriteBaseline,
                "baseline.reversible-local-write",
            ),
            RiskClass::ReversibleLocalWrite => confirmation(
                PolicyReason::ReversibleLocalWriteRequiresConfirmation,
                "baseline.reversible-write-confirm",
                ConfirmationLevel::Standard,
            ),
            RiskClass::ExternalSideEffect => confirmation(
                PolicyReason::ExternalSideEffectRequiresConfirmation,
                "baseline.external-side-effect-confirm",
                ConfirmationLevel::Sensitive,
            ),
            RiskClass::Destructive => confirmation(
                PolicyReason::DestructiveRequiresConfirmation,
                "baseline.destructive-confirm",
                ConfirmationLevel::Destructive,
            ),
            RiskClass::Privileged | RiskClass::Unknown => unreachable!("handled before baseline"),
        }
    }
}

fn default_arguments(capability: &CapabilityDescriptor) -> halquen_domain::ActionArguments {
    match capability.arguments {
        halquen_domain::ActionArgumentKind::None => halquen_domain::ActionArguments::None,
        halquen_domain::ActionArgumentKind::OpenApp => halquen_domain::ActionArguments::OpenApp {
            app: halquen_domain::EntityId::new("app:policy-evaluation")
                .expect("static policy entity is valid"),
        },
    }
}

fn has_resource(proposal: &ActionProposal, class: ResourceClassification) -> bool {
    proposal
        .context
        .resources
        .iter()
        .any(|resource| resource.classification == class)
}

fn secret_to_external(proposal: &ActionProposal) -> bool {
    proposal.context.data_flow.as_ref().is_some_and(|flow| {
        (flow.data_classification == DataClassification::Secret
            || flow.source.classification == ResourceClassification::Secret)
            && matches!(
                flow.destination,
                DestinationClass::External | DestinationClass::UntrustedExternal
            )
    })
}

fn blocked(reason: PolicyReason, rule_id: &str, hard_deny: bool) -> PolicyEvaluation {
    PolicyEvaluation::blocked(decision(
        PolicyOutcome::Deny,
        reason,
        rule_id,
        hard_deny,
        None,
        None,
    ))
}

fn allow(reason: PolicyReason, rule_id: &str) -> PolicyDecision {
    decision(PolicyOutcome::Allow, reason, rule_id, false, None, None)
}

fn confirmation(reason: PolicyReason, rule_id: &str, level: ConfirmationLevel) -> PolicyDecision {
    decision(
        PolicyOutcome::Confirm,
        reason,
        rule_id,
        false,
        Some(level),
        None,
    )
}

fn decision(
    outcome: PolicyOutcome,
    reason: PolicyReason,
    rule_id: &str,
    hard_deny: bool,
    confirmation_level: Option<ConfirmationLevel>,
    expires_at_ms: Option<i64>,
) -> PolicyDecision {
    PolicyDecision {
        outcome,
        reason,
        rule_id: rule_id.to_owned(),
        hard_deny,
        confirmation_level,
        expires_at_ms,
    }
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use halquen_domain::{
        ActionArgumentKind, ActionArguments, AgentExecutionIdentity, AgentId, AgentInstanceId,
        AgentSessionId, CapabilityId, DataFlowContext, EntityId, PermissionId, PermissionLifetime,
        PermissionScope, PermissionSessionScope, ResourceDescriptor, ResourceKind,
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

    fn proposal(capability: &CapabilityDescriptor, context: ActionContext) -> ActionProposal {
        ActionProposal::new(
            ActionRequest::new(capability.id.clone(), ActionArguments::None),
            context,
        )
        .unwrap()
    }

    #[test]
    fn hard_deny_secret_to_external_cannot_be_confirmed() {
        let descriptor = descriptor(RiskClass::ExternalSideEffect);
        let secret = ResourceDescriptor {
            kind: ResourceKind::File,
            identifier: "fixture:ssh-private-key".to_owned(),
            classification: ResourceClassification::Secret,
        };
        let context = ActionContext::untrusted(ActionOrigin::ExternalContent, None)
            .unwrap()
            .with_resource(secret.clone())
            .with_data_flow(DataFlowContext {
                source: secret,
                data_classification: DataClassification::Secret,
                destination: DestinationClass::UntrustedExternal,
            });
        let proposal = proposal(&descriptor, context);
        let evaluation = PolicyEngine::new().authorize_confirmed_proposal_once(
            &descriptor,
            proposal,
            ExecutionId::generate(),
            &PolicyContext::default(),
        );
        assert_eq!(evaluation.decision.outcome, PolicyOutcome::Deny);
        assert!(evaluation.decision.hard_deny);
        assert!(evaluation.authorization().is_none());
    }

    #[test]
    fn destructive_production_action_is_hard_denied() {
        let descriptor = descriptor(RiskClass::Destructive);
        let context = ActionContext::trusted_user(None).with_resource(ResourceDescriptor {
            kind: ResourceKind::Database,
            identifier: "database:production".to_owned(),
            classification: ResourceClassification::Production,
        });
        let result = PolicyEngine::new().authorize_proposal(
            &descriptor,
            proposal(&descriptor, context),
            ExecutionId::generate(),
            &PolicyContext::default(),
        );
        assert_eq!(
            result.decision.reason,
            PolicyReason::ImmutableProductionDestructiveDeny
        );
    }

    #[test]
    fn permission_is_exact_to_the_action() {
        let mut descriptor = descriptor(RiskClass::ExternalSideEffect);
        descriptor.arguments = ActionArgumentKind::OpenApp;
        let make = |entity: &str| {
            ActionProposal::new(
                ActionRequest::new(
                    descriptor.id.clone(),
                    ActionArguments::OpenApp {
                        app: EntityId::new(entity).unwrap(),
                    },
                ),
                ActionContext::trusted_user(None),
            )
            .unwrap()
        };
        let telegram = make("app:telegram");
        let discord = make("app:discord");
        let grant = PermissionGrant {
            id: PermissionId::generate(),
            effect: PermissionEffect::Allow,
            lifetime: PermissionLifetime::Always,
            scope: PermissionScope::from_proposal(&telegram),
            session: None,
            agent_id: None,
            granted_by: ActionOrigin::UserExplicit,
            granted_at_ms: 0,
            expires_at_ms: None,
            revoked_at_ms: None,
            use_limit: None,
            use_count: 0,
        };
        let mut context = PolicyContext::default();
        context.add_permission_grant(grant);
        assert_eq!(
            PolicyEngine::new()
                .evaluate_proposal(&descriptor, &telegram, &context)
                .outcome,
            PolicyOutcome::Allow
        );
        assert_eq!(
            PolicyEngine::new()
                .evaluate_proposal(&descriptor, &discord, &context)
                .outcome,
            PolicyOutcome::Confirm
        );
    }

    #[test]
    fn explicit_confirmation_never_overrides_privileged_deny() {
        let descriptor = descriptor(RiskClass::Privileged);
        let action = ActionRequest::new(descriptor.id.clone(), ActionArguments::None);
        let result = PolicyEngine::new().authorize_confirmed_once(
            &descriptor,
            action,
            ExecutionId::generate(),
            &PolicyContext::default(),
        );
        assert_eq!(result.decision.outcome, PolicyOutcome::Deny);
        assert!(result.authorization().is_none());
    }

    #[test]
    fn ai_proposal_never_creates_authorization_from_confidence_or_origin() {
        let descriptor = descriptor(RiskClass::ExternalSideEffect);
        let context =
            ActionContext::untrusted(ActionOrigin::AiProposal, Some("model:fixture".to_owned()))
                .unwrap();
        let result = PolicyEngine::new().authorize_proposal(
            &descriptor,
            proposal(&descriptor, context),
            ExecutionId::generate(),
            &PolicyContext::default(),
        );
        assert_eq!(result.decision.outcome, PolicyOutcome::Confirm);
        assert!(result.authorization().is_none());
    }

    fn agent_identity(agent_id: AgentId, session_id: AgentSessionId) -> AgentExecutionIdentity {
        AgentExecutionIdentity {
            agent_id,
            instance_id: AgentInstanceId::generate(),
            session_id,
        }
    }

    #[test]
    fn agent_side_effect_requires_confirmation_without_an_exact_agent_grant() {
        let descriptor = descriptor(RiskClass::LocalSideEffect);
        let identity = agent_identity(AgentId::generate(), AgentSessionId::generate());
        let proposal = proposal(&descriptor, ActionContext::agent(identity));
        let result = PolicyEngine::new().authorize_proposal(
            &descriptor,
            proposal,
            ExecutionId::generate(),
            &PolicyContext::default(),
        );
        assert_eq!(
            result.decision.reason,
            PolicyReason::AgentSideEffectRequiresConfirmation
        );
        assert!(result.authorization().is_none());
    }

    #[test]
    fn agent_grant_is_exact_to_agent_and_live_session() {
        let descriptor = descriptor(RiskClass::LocalSideEffect);
        let agent_id = AgentId::generate();
        let session_id = AgentSessionId::generate();
        let identity = agent_identity(agent_id.clone(), session_id.clone());
        let agent_proposal = proposal(&descriptor, ActionContext::agent(identity));
        let grant = PermissionGrant {
            id: PermissionId::generate(),
            effect: PermissionEffect::Allow,
            lifetime: PermissionLifetime::Session,
            scope: PermissionScope::from_proposal(&agent_proposal),
            session: Some(PermissionSessionScope::Agent(session_id.clone())),
            agent_id: Some(agent_id.clone()),
            granted_by: ActionOrigin::UserExplicit,
            granted_at_ms: 0,
            expires_at_ms: None,
            revoked_at_ms: None,
            use_limit: None,
            use_count: 0,
        };

        let mut live = PolicyContext::default();
        live.set_action_context(agent_proposal.context.clone());
        live.add_permission_grant(grant.clone());
        assert_eq!(
            PolicyEngine::new()
                .evaluate_proposal(&descriptor, &agent_proposal, &live)
                .outcome,
            PolicyOutcome::Allow
        );

        let other_session = proposal(
            &descriptor,
            ActionContext::agent(agent_identity(agent_id, AgentSessionId::generate())),
        );
        let mut stale = PolicyContext::default();
        stale.set_action_context(other_session.context.clone());
        stale.add_permission_grant(grant.clone());
        assert_eq!(
            PolicyEngine::new()
                .evaluate_proposal(&descriptor, &other_session, &stale)
                .outcome,
            PolicyOutcome::Confirm
        );

        let other_agent = proposal(
            &descriptor,
            ActionContext::agent(agent_identity(AgentId::generate(), session_id)),
        );
        let mut wrong_agent = PolicyContext::default();
        wrong_agent.set_action_context(other_agent.context.clone());
        wrong_agent.add_permission_grant(grant);
        assert_eq!(
            PolicyEngine::new()
                .evaluate_proposal(&descriptor, &other_agent, &wrong_agent)
                .outcome,
            PolicyOutcome::Confirm
        );
    }
}
