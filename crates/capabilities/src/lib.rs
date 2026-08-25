#![forbid(unsafe_code)]

pub mod builtins;
pub mod executor;
pub mod registry;

pub use builtins::open_app_descriptor;
pub use executor::{
    DryRunExecutor,
    ExecutionError,
    ExecutionOutcome,
    ExecutionResultCode,
    Executor,
};

pub use registry::{
    CapabilityRegistry,
    RegistryError,
};
