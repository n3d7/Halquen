#![forbid(unsafe_code)]

mod chat;
mod interaction_service;
pub mod ipc;
mod logging;
pub mod service;

pub use ipc::{DaemonError, run};
pub use service::HalquenService;
