//! Typed command and event contract between the Reqly UI and core.
//!
//! Every operation the UI can perform is a [`Command`] with serde-typed input
//! and output. Desktop (Tauri) and server (HTTP) only forward JSON to
//! [`Registry::dispatch`], so both platforms share one implementation and one
//! contract test suite.

pub mod access;
pub mod commands;
#[cfg(feature = "contract")]
pub mod contract;
pub mod events;
pub mod registry;
pub mod ts;

pub use events::{Event, EventBus};
pub use registry::{Command, Registry};
