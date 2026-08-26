#![forbid(unsafe_code)]

pub mod ipc;
pub mod service;
mod chat;
mod interaction_service;
mod logging;

pub use ipc::{DaemonError, run};
pub use service::HalquenService;
