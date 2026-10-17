//! HTTP for Reqly: turns stored requests into wire requests ([`prepare`])
//! and executes them with libcurl ([`engine`]), see ADR-002.

pub mod auth;
mod ca;
pub mod curl;
pub mod engine;
pub mod filter;
pub mod policy;
pub mod prepare;
pub mod snapshot;
