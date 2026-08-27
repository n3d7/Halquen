#![forbid(unsafe_code)]

pub mod applications;
pub mod builtins;
pub mod executable;
pub mod executor;
pub mod registry;

pub use applications::{ApplicationRegistry, ApplicationRegistryError, SharedApplicationRegistry};
pub use builtins::open_app_descriptor;
pub use executable::{ExecutableIdentityError, inspect_executable, verify_executable};
pub use executor::{
    DryRunExecutor, ExecutionError, ExecutionOutcome, ExecutionResultCode, Executor,
    RealLinuxExecutor, RuntimeExecutor,
};

pub use registry::{CapabilityRegistry, RegistryError};
