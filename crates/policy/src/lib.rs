#![forbid(unsafe_code)]

pub mod decision;
pub mod engine;

pub use decision::{
    ExecutionAuthorization, PolicyDecision, PolicyEvaluation, PolicyOutcome, PolicyReason,
};
pub use engine::{PolicyContext, PolicyEngine};
