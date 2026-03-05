//! ChoirOS Sandbox - Actor-based backend with REST API
//!
//! This crate provides the backend server for ChoirOS, implementing
//! an actor-based architecture with event sourcing.

#![allow(clippy::too_many_arguments)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::result_large_err)]
#![allow(clippy::type_complexity)]

pub mod actors;
pub mod api;
pub mod app_state;
#[allow(clippy::all)]
pub mod baml_client;
pub mod markdown;
pub mod observability;
pub mod runtime_env;
pub mod tools;

pub mod supervisor;
