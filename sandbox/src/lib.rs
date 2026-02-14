//! ChoirOS Sandbox - Actor-based backend with REST API
//!
//! This crate provides the backend server for ChoirOS, implementing
//! an actor-based architecture with event sourcing.

pub mod actors;
pub mod api;
pub mod app_state;
pub mod baml_client;
pub mod markdown;
pub mod observability;
pub mod runtime_env;
pub mod tools;

pub mod supervisor;
