#![forbid(unsafe_code)]

mod agent_service;
mod chat;
mod interaction_service;
pub mod ipc;
mod logging;
mod security_service;
pub mod service;

pub use ipc::{DaemonError, DaemonOptions, ExecutionMode, run};
pub use service::HalquenService;
