use std::collections::{BTreeMap, BTreeSet};

use halquen_domain::{ActionRequest, CapabilityId, EvidenceId};
use serde::{Deserialize, Serialize};

use crate::Evidence;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProceduralCandidate {
    pub name: String,
    pub capabilities: Vec<CapabilityId>,
    pub steps: Vec<ActionRequest>,
    pub evidence_ids: Vec<EvidenceId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromotionDecision {
    EligibleForPolicyReview,
    InsufficientIndependentAuthority,
    InvalidCandidate,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ProceduralPromotionValidator;

impl ProceduralPromotionValidator {
    pub fn evaluate(
        &self,
        candidate: &ProceduralCandidate,
        evidence: &[Evidence],
    ) -> PromotionDecision {
        if candidate.name.trim().is_empty()
            || candidate.capabilities.is_empty()
            || candidate.steps.is_empty()
            || candidate.evidence_ids.is_empty()
            || candidate
                .steps
                .iter()
                .any(|step| !candidate.capabilities.contains(&step.capability_id))
        {
            return PromotionDecision::InvalidCandidate;
        }

        let referenced: BTreeSet<_> = candidate.evidence_ids.iter().collect();
        let mut supplied = BTreeMap::new();
        for item in evidence {
            if supplied.insert(&item.id, item).is_some() {
                return PromotionDecision::InvalidCandidate;
            }
        }
        if referenced.len() != candidate.evidence_ids.len()
            || supplied.len() != referenced.len()
            || referenced.iter().any(|id| !supplied.contains_key(id))
        {
            return PromotionDecision::InvalidCandidate;
        }

        if candidate.evidence_ids.iter().any(|id| {
            supplied[id]
                .trust
                .independently_authorizes_procedural_memory()
        })
        {
            PromotionDecision::EligibleForPolicyReview
        } else {
            PromotionDecision::InsufficientIndependentAuthority
        }
    }
}

#[cfg(test)]
mod tests {
    use halquen_domain::{
        ActionArguments, ActionRequest, CapabilityId, EvidenceId, TrustClass,
    };

    use super::*;

    fn candidate(evidence_ids: Vec<EvidenceId>) -> ProceduralCandidate {
        let id = CapabilityId::new("timer.start").unwrap();
        ProceduralCandidate {
            name: "Start a timer".to_owned(),
            capabilities: vec![id.clone()],
            steps: vec![ActionRequest::new(id, ActionArguments::None)],
            evidence_ids,
        }
    }

    fn evidence(trust: TrustClass) -> Evidence {
        Evidence {
            id: EvidenceId::generate(),
            trust,
            source_reference: None,
            created_at_ms: 1,
        }
    }

    #[test]
    fn untrusted_evidence_cannot_promote_procedural_memory() {
        let items = vec![
            evidence(TrustClass::ExternalContent),
            evidence(TrustClass::AiInferred),
            evidence(TrustClass::AiInferred),
            evidence(TrustClass::PluginAsserted),
        ];
        let candidate = candidate(items.iter().map(|item| item.id.clone()).collect());
        assert_eq!(
            ProceduralPromotionValidator.evaluate(&candidate, &items),
            PromotionDecision::InsufficientIndependentAuthority
        );
    }

    #[test]
    fn explicit_or_confirmed_evidence_allows_policy_review() {
        for trust in [TrustClass::UserExplicit, TrustClass::UserConfirmedResult] {
            let item = evidence(trust);
            assert_eq!(
                ProceduralPromotionValidator.evaluate(
                    &candidate(vec![item.id.clone()]),
                    &[item],
                ),
                PromotionDecision::EligibleForPolicyReview
            );
        }
    }

    #[test]
    fn behaviour_and_local_verification_are_support_not_authority() {
        let items = vec![
            evidence(TrustClass::UserBehaviour),
            evidence(TrustClass::LocalVerified),
        ];
        let candidate = candidate(items.iter().map(|item| item.id.clone()).collect());
        assert_eq!(
            ProceduralPromotionValidator.evaluate(&candidate, &items),
            PromotionDecision::InsufficientIndependentAuthority
        );
    }

    #[test]
    fn unrelated_trusted_evidence_is_rejected() {
        let referenced = evidence(TrustClass::ExternalContent);
        let unrelated = evidence(TrustClass::UserExplicit);
        assert_eq!(
            ProceduralPromotionValidator.evaluate(
                &candidate(vec![referenced.id.clone()]),
                &[referenced, unrelated],
            ),
            PromotionDecision::InvalidCandidate
        );
    }
}
