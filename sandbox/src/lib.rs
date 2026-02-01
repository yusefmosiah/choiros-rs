//! ChoirOS Sandbox - Actor-based backend with REST API
//!
//! This crate provides the backend server for ChoirOS, implementing
//! an actor-based architecture with event sourcing.

pub mod actors;
pub mod actor_manager;
pub mod api;
pub mod baml_client;
pub mod tools;
