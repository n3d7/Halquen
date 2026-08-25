use halquen_domain::{
    ActionArgumentKind, CapabilityDescriptor, CapabilityId, ConfirmationPolicy, RiskClass,
};

pub fn open_app_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor {
        id: CapabilityId::new("system.open_app").expect("built-in capability ID is valid"),
        version: 1,
        description: "Open an installed application".to_owned(),
        risk: RiskClass::LocalSideEffect,
        side_effect: true,
        idempotent: false,
        reversible: false,
        scope_requirements: Vec::new(),
        confirmation: ConfirmationPolicy::RiskBased,
        timeout_ms: 5_000,
        arguments: ActionArgumentKind::OpenApp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_app_is_a_valid_non_reversible_local_side_effect() {
        let descriptor = open_app_descriptor();
        assert_eq!(descriptor.risk, RiskClass::LocalSideEffect);
        assert!(descriptor.side_effect);
        assert!(!descriptor.reversible);
        descriptor.validate().unwrap();
    }
}
