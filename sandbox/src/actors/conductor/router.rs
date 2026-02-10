//! ConductorActor worker routing logic
//!
//! Routing control is typed and explicit. No natural-language matching is used
//! for workflow authority.

use shared_types::{ConductorExecuteRequest, ConductorWorkerStep};

/// Routing decision for a task
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    /// Ordered worker plan to execute
    pub plan: Vec<ConductorWorkerStep>,
    /// Reason for the routing decision
    pub reason: String,
}

/// Router for determining which workers handle a task
pub struct WorkerRouter;

impl WorkerRouter {
    /// Create a new router
    pub fn new() -> Self {
        Self
    }

    /// Route a request into an ordered typed worker plan.
    pub fn route(&self, request: &ConductorExecuteRequest) -> Result<RoutingDecision, String> {
        if let Some(plan) = &request.worker_plan {
            if plan.is_empty() {
                return Err("worker_plan cannot be empty".to_string());
            }
            return Ok(RoutingDecision {
                plan: plan.clone(),
                reason: "Using explicit typed worker_plan from request".to_string(),
            });
        }

        // No explicit plan provided. ConductorActor applies capability-aware
        // default policy at execution time.
        Ok(RoutingDecision {
            plan: Vec::new(),
            reason: "No explicit worker_plan provided".to_string(),
        })
    }
}

impl Default for WorkerRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_types::{ConductorOutputMode, ConductorWorkerType};

    #[test]
    fn test_route_without_plan_defers_to_conductor_policy() {
        let router = WorkerRouter::new();
        let request = ConductorExecuteRequest {
            objective: "Research Rust async patterns".to_string(),
            desktop_id: "desktop-1".to_string(),
            output_mode: ConductorOutputMode::MarkdownReportToWriter,
            worker_plan: None,
            hints: None,
            correlation_id: None,
        };

        let decision = router.route(&request).expect("route should succeed");
        assert!(decision.plan.is_empty());
        assert_eq!(decision.reason, "No explicit worker_plan provided");
    }

    #[test]
    fn test_route_explicit_plan() {
        let router = WorkerRouter::new();
        let request = ConductorExecuteRequest {
            objective: "run".to_string(),
            desktop_id: "desktop-1".to_string(),
            output_mode: ConductorOutputMode::MarkdownReportToWriter,
            worker_plan: Some(vec![ConductorWorkerStep {
                worker_type: ConductorWorkerType::Terminal,
                objective: Some("List files".to_string()),
                terminal_command: Some("ls -la".to_string()),
                timeout_ms: Some(5_000),
                max_results: None,
                max_steps: Some(1),
            }]),
            hints: None,
            correlation_id: None,
        };

        let decision = router.route(&request).expect("route should succeed");
        assert_eq!(decision.plan.len(), 1);
        assert_eq!(decision.plan[0].worker_type, ConductorWorkerType::Terminal);
    }
}
