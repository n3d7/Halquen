use halquen_domain::{
    AiModel, AiTaskType, ApplicationSettings, ModelId, ModelSelection, PrivacyClass, Provider,
    ProviderId, ProviderKind, RoutingPreset,
};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteRequest {
    pub task: AiTaskType,
    pub selection: ModelSelection,
    pub contains_personal_context: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedModel {
    pub provider_id: ProviderId,
    pub model_id: ModelId,
    pub reason_code: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RouteError {
    #[error("AI model calls are disabled")]
    AiDisabled,
    #[error("the selected model is unavailable or forbidden by privacy policy")]
    SelectedModelIneligible,
    #[error("no eligible provider and model are configured")]
    NoEligibleModel,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ModelRouter;

impl ModelRouter {
    pub fn select(
        &self,
        settings: &ApplicationSettings,
        providers: &[Provider],
        models: &[AiModel],
        request: &RouteRequest,
    ) -> Result<SelectedModel, RouteError> {
        if settings.max_model_calls_per_request == 0 {
            return Err(RouteError::AiDisabled);
        }
        let eligible = |model: &&AiModel| {
            model.enabled
                && model.task_eligibility.contains(&request.task)
                && providers.iter().any(|provider| {
                    provider.id == model.provider_id
                        && provider.enabled
                        && provider.configured
                        && provider_supported(provider.kind)
                        && provider.privacy == model.privacy
                        && privacy_allowed(settings, provider.privacy, request.contains_personal_context)
                })
        };

        if let ModelSelection::Model { model_id } = &request.selection {
            let model = models
                .iter()
                .find(|model| &model.id == model_id)
                .filter(eligible)
                .ok_or(RouteError::SelectedModelIneligible)?;
            return Ok(SelectedModel {
                provider_id: model.provider_id.clone(),
                model_id: model.id.clone(),
                reason_code: "manual_selection_policy_validated",
            });
        }

        let mut candidates: Vec<_> = models.iter().filter(eligible).collect();
        candidates.sort_by_key(|model| route_score(settings.routing_preset, model));
        let model = candidates.last().ok_or(RouteError::NoEligibleModel)?;
        Ok(SelectedModel {
            provider_id: model.provider_id.clone(),
            model_id: model.id.clone(),
            reason_code: "automatic_deterministic_route",
        })
    }
}

fn provider_supported(kind: ProviderKind) -> bool {
    matches!(
        kind,
        ProviderKind::OpenAiCompatible
            | ProviderKind::OpenAi
            | ProviderKind::Ollama
            | ProviderKind::LmStudio
    )
}

fn privacy_allowed(
    settings: &ApplicationSettings,
    privacy: PrivacyClass,
    contains_personal_context: bool,
) -> bool {
    match privacy {
        PrivacyClass::Local => settings.allow_local_ai,
        PrivacyClass::Cloud => {
            settings.allow_cloud_ai
                && (!contains_personal_context || settings.allow_personal_context)
        }
    }
}

fn route_score(preset: RoutingPreset, model: &AiModel) -> i32 {
    let local_bonus = i32::from(model.privacy == PrivacyClass::Local) * 1_000;
    let default_bonus = i32::from(model.is_default) * 100;
    let priority = i32::from(model.priority);
    match preset {
        RoutingPreset::MinimizeAiUsage | RoutingPreset::PreferLocal => {
            local_bonus + default_bonus + priority
        }
        RoutingPreset::MinimizeCost => local_bonus / 2 + default_bonus + priority,
        RoutingPreset::PreferQuality => default_bonus + priority * 2,
        RoutingPreset::Balanced | RoutingPreset::Custom => {
            local_bonus / 4 + default_bonus + priority
        }
    }
}

#[cfg(test)]
mod tests {
    use halquen_domain::{ProviderStatus, ProviderKind};

    use super::*;

    fn provider(privacy: PrivacyClass) -> Provider {
        Provider {
            id: ProviderId::generate(),
            kind: ProviderKind::OpenAiCompatible,
            name: "test".to_owned(),
            base_url: "https://example.invalid/v1".to_owned(),
            enabled: true,
            privacy,
            configured: true,
            credential_id: Some("credential:test".to_owned()),
            status: ProviderStatus::Configured,
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    fn model(provider: &Provider) -> AiModel {
        AiModel {
            id: ModelId::generate(),
            provider_id: provider.id.clone(),
            display_name: "model".to_owned(),
            provider_model_id: "model".to_owned(),
            enabled: true,
            context_limit: Some(8_192),
            privacy: provider.privacy,
            priority: 0,
            task_eligibility: vec![AiTaskType::Conversation],
            is_default: true,
        }
    }

    #[test]
    fn cloud_disabled_cannot_be_bypassed_by_manual_selection() {
        let provider = provider(PrivacyClass::Cloud);
        let model = model(&provider);
        let settings = ApplicationSettings::default();
        let result = ModelRouter.select(
            &settings,
            &[provider],
            &[model.clone()],
            &RouteRequest {
                task: AiTaskType::Conversation,
                selection: ModelSelection::Model {
                    model_id: model.id,
                },
                contains_personal_context: false,
            },
        );
        assert_eq!(result, Err(RouteError::SelectedModelIneligible));
    }

    #[test]
    fn zero_model_call_budget_disables_ai_before_route_selection() {
        let provider = provider(PrivacyClass::Local);
        let model = model(&provider);
        let mut settings = ApplicationSettings::default();
        settings.max_model_calls_per_request = 0;
        let result = ModelRouter.select(
            &settings,
            &[provider],
            &[model],
            &RouteRequest {
                task: AiTaskType::Conversation,
                selection: ModelSelection::Automatic,
                contains_personal_context: false,
            },
        );
        assert_eq!(result, Err(RouteError::AiDisabled));
    }

    #[test]
    fn prefer_local_routes_to_local_provider() {
        let local = provider(PrivacyClass::Local);
        let cloud = provider(PrivacyClass::Cloud);
        let local_model = model(&local);
        let cloud_model = model(&cloud);
        let mut settings = ApplicationSettings::default();
        settings.allow_cloud_ai = true;
        settings.routing_preset = RoutingPreset::PreferLocal;
        let selected = ModelRouter
            .select(
                &settings,
                &[local, cloud],
                &[cloud_model, local_model.clone()],
                &RouteRequest {
                    task: AiTaskType::Conversation,
                    selection: ModelSelection::Automatic,
                    contains_personal_context: false,
                },
            )
            .unwrap();
        assert_eq!(selected.model_id, local_model.id);
    }
}
