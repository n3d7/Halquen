use halquen_capabilities::{
    CapabilityRegistry,
    DryRunExecutor,
};

use halquen_domain::{
    ActionArguments,
    ActionRequest,
    CapabilityDescriptor,
    CapabilityId,
    EntityId,
    RiskClass,
};

use halquen_policy::{
    PolicyDecision,
    PolicyEngine,
};

fn main() {
    let mut registry = CapabilityRegistry::new();
    let policy = PolicyEngine::new();
    let executor = DryRunExecutor::new();

    let open_app = CapabilityDescriptor {
        id: CapabilityId::new("system.open_app"),
        version: 1,
        description: "Open an installed application".to_string(),
        risk: RiskClass::ReversibleLocalWrite,
        side_effect: true,
        reversible: false,
    };

    registry
        .register(open_app)
        .expect("failed to register capability");

    let capability_id = CapabilityId::new("system.open_app");

    let capability = registry
        .get(&capability_id)
        .expect("capability not found");

    let request = ActionRequest::new(
        capability_id,
        ActionArguments::OpenApp {
            app: EntityId::new("app:telegram"),
        },
    );

    let decision = policy.evaluate(capability);

    match decision {
        PolicyDecision::Allow => {
            let receipt = executor.execute(&request);

            println!("{}", receipt.message);
        }

        PolicyDecision::Confirm => {
            println!("User confirmation required");
        }

        PolicyDecision::Deny => {
            println!("Execution denied");
        }
    }
}