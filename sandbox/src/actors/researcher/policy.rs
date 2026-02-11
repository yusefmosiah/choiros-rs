//! Researcher policy module - DEPRECATED
//!
//! This module previously contained the researcher-specific planning logic.
//! The ResearcherActor now uses the unified agent harness (agent_harness module)
//! which provides a generic DECIDE -> EXECUTE -> loop pattern.
//!
//! The researcher-specific BAML types (ResearcherPlanInput, ResearcherPlanOutput,
//! ResearchAction, ResearchStatus) are kept for backward compatibility but are
//! no longer used by the harness.

// Re-export the BAML-generated types for any code that may reference them
pub use crate::baml_client::types::{ResearchAction, ResearchStatus};
