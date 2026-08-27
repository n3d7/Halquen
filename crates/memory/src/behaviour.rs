use std::collections::BTreeMap;

use halquen_domain::{BehaviourOutcome, EntityId, IntentCandidate, IntentUsageEvent};

pub const DEFAULT_HALF_LIFE_MS: i64 = 7 * 24 * 60 * 60 * 1_000;
pub const DEFAULT_RETENTION_MS: i64 = 90 * 24 * 60 * 60 * 1_000;
pub const DEFAULT_MAX_EVENTS: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntentResolution {
    Resolved(IntentCandidate),
    Ambiguous(Vec<IntentCandidate>),
    Unknown,
}

#[derive(Debug, Clone)]
pub struct BehaviourScorer {
    half_life_ms: i64,
    resolve_threshold_permille: u16,
    minimum_gap_permille: u16,
}

impl BehaviourScorer {
    pub fn new(half_life_ms: i64) -> Self {
        Self {
            half_life_ms: half_life_ms.max(1),
            resolve_threshold_permille: 650,
            minimum_gap_permille: 120,
        }
    }

    pub fn score(&self, events: &[IntentUsageEvent], now_ms: i64) -> Vec<IntentCandidate> {
        let mut scores = BTreeMap::<EntityId, f64>::new();
        for event in events.iter().take(DEFAULT_MAX_EVENTS) {
            let age_ms = now_ms.saturating_sub(event.created_at_ms).max(0);
            if age_ms > DEFAULT_RETENTION_MS {
                continue;
            }
            let decay = 2_f64.powf(-(age_ms as f64) / (self.half_life_ms as f64));
            let signal = match event.outcome {
                BehaviourOutcome::Success => 1.25,
                BehaviourOutcome::Failure => -0.75,
                BehaviourOutcome::CorrectionAccepted => 3.0,
                BehaviourOutcome::CorrectionRejected => -4.0,
            };
            *scores.entry(event.entity_id.clone()).or_default() += decay * signal;
        }
        let mut candidates = scores
            .into_iter()
            .filter_map(|(entity_id, raw)| {
                let positive = raw.max(0.0);
                if positive == 0.0 {
                    return None;
                }
                let confidence = (1.0 - (-positive / 3.0).exp()) * 1_000.0;
                Some(IntentCandidate {
                    entity_id,
                    score_permille: confidence.round().clamp(0.0, 1_000.0) as u16,
                })
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            right
                .score_permille
                .cmp(&left.score_permille)
                .then_with(|| left.entity_id.cmp(&right.entity_id))
        });
        candidates
    }

    pub fn resolve(&self, events: &[IntentUsageEvent], now_ms: i64) -> IntentResolution {
        let candidates = self.score(events, now_ms);
        let Some(first) = candidates.first() else {
            return IntentResolution::Unknown;
        };
        let runner_up = candidates
            .get(1)
            .map_or(0, |candidate| candidate.score_permille);
        if first.score_permille >= self.resolve_threshold_permille
            && first.score_permille.saturating_sub(runner_up) >= self.minimum_gap_permille
        {
            IntentResolution::Resolved(first.clone())
        } else {
            IntentResolution::Ambiguous(candidates.into_iter().take(5).collect())
        }
    }
}

impl Default for BehaviourScorer {
    fn default() -> Self {
        Self::new(DEFAULT_HALF_LIFE_MS)
    }
}

#[cfg(test)]
mod tests {
    use halquen_domain::BehaviourEventId;

    use super::*;

    fn event(entity: &str, outcome: BehaviourOutcome, created_at_ms: i64) -> IntentUsageEvent {
        IntentUsageEvent {
            id: BehaviourEventId::generate(),
            intent: "open_application".to_owned(),
            entity_id: EntityId::new(entity).unwrap(),
            outcome,
            context_class: "application".to_owned(),
            created_at_ms,
        }
    }

    #[test]
    fn recent_actions_outweigh_many_old_actions() {
        let now = 1_800_000_000_000_i64;
        let six_months = 180 * 24 * 60 * 60 * 1_000_i64;
        let mut events = (0..200)
            .map(|_| event("app:telegram", BehaviourOutcome::Success, now - six_months))
            .collect::<Vec<_>>();
        events.extend((0..3).map(|_| event("app:discord", BehaviourOutcome::Success, now - 1_000)));
        let candidates = BehaviourScorer::default().score(&events, now);
        assert_eq!(candidates[0].entity_id.as_str(), "app:discord");
    }

    #[test]
    fn close_scores_require_clarification() {
        let now = 1_800_000_000_000_i64;
        let events = vec![
            event("app:telegram", BehaviourOutcome::Success, now),
            event("app:discord", BehaviourOutcome::Success, now),
        ];
        assert!(matches!(
            BehaviourScorer::default().resolve(&events, now),
            IntentResolution::Ambiguous(_)
        ));
    }

    #[test]
    fn correction_changes_future_ranking() {
        let now = 1_800_000_000_000_i64;
        let events = vec![
            event("app:telegram", BehaviourOutcome::Success, now - 10),
            event("app:telegram", BehaviourOutcome::CorrectionRejected, now),
            event("app:discord", BehaviourOutcome::CorrectionAccepted, now),
        ];
        let candidates = BehaviourScorer::default().score(&events, now);
        assert_eq!(candidates[0].entity_id.as_str(), "app:discord");
        assert!(
            candidates
                .iter()
                .all(|item| item.entity_id.as_str() != "app:telegram")
        );
    }
}
