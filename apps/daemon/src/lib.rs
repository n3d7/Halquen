#![forbid(unsafe_code)]

pub mod ipc;
pub mod service;

pub use ipc::{DaemonError, run};
pub use service::HalquenService;
