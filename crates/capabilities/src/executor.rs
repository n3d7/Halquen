use halquen_domain::{
    ActionRequest,
    CapabilityId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionReceipt {
    pub capability_id: CapabilityId,
    pub message: String,
}

pub struct DryRunExecutor;

impl DryRunExecutor {
    pub fn new() -> Self {
        Self
    }

    pub fn execute(
        &self,
        request: &ActionRequest,
    ) -> ExecutionReceipt {
        ExecutionReceipt {
            capability_id: request.capability_id.clone(),
            message: format!(
                "Simulated execution of {}",
                request.capability_id.as_str()
            ),
        }
    }
}

impl Default for DryRunExecutor {
    fn default() -> Self {
        Self::new()
    }
}