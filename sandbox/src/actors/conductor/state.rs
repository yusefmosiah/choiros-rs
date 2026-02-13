//! ConductorActor state management
//!
//! Manages the lifecycle of runs, agenda items, capability calls, artifacts, and decisions.
//! Implements the agentic runtime model with wake/display event lane separation.

use shared_types::{
    AgendaItemStatus, CapabilityCallStatus, ConductorAgendaItem, ConductorArtifact,
    ConductorCapabilityCall, ConductorDecision, ConductorRunState, ConductorRunStatus,
    ConductorTaskState, ConductorToastPayload,
};
use std::collections::HashMap;

/// State container for ConductorActor - new runtime model
pub struct ConductorState {
    /// Legacy task tracking (for compatibility)
    tasks: HashMap<String, ConductorTaskState>,

    /// New runtime model: runs indexed by run_id
    runs: HashMap<String, ConductorRunState>,

    /// Active capability calls by call_id (for quick lookup)
    active_calls: HashMap<String, (String, ConductorCapabilityCall)>, // call_id -> (run_id, call)
}

impl ConductorState {
    /// Create a new empty state
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            runs: HashMap::new(),
            active_calls: HashMap::new(),
        }
    }

    // =========================================================================
    // Legacy Task API (for compatibility)
    // =========================================================================

    /// Insert a new task in Queued status
    pub fn insert_task(
        &mut self,
        task: ConductorTaskState,
    ) -> Result<(), super::protocol::ConductorError> {
        if self.tasks.contains_key(&task.task_id) {
            return Err(super::protocol::ConductorError::DuplicateTask(
                task.task_id.clone(),
            ));
        }
        self.tasks.insert(task.task_id.clone(), task);
        Ok(())
    }

    /// Get a task by ID
    pub fn get_task(&self, task_id: &str) -> Option<&ConductorTaskState> {
        self.tasks.get(task_id)
    }

    /// Get a mutable task by ID
    pub fn get_task_mut(&mut self, task_id: &str) -> Option<&mut ConductorTaskState> {
        self.tasks.get_mut(task_id)
    }

    /// Transition a task to Running status
    pub fn transition_to_running(
        &mut self,
        task_id: &str,
    ) -> Result<&ConductorTaskState, super::protocol::ConductorError> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| super::protocol::ConductorError::NotFound(task_id.to_string()))?;

        task.status = shared_types::ConductorTaskStatus::Running;
        task.updated_at = chrono::Utc::now();

        Ok(&*task)
    }

    /// Transition a task to WaitingWorker status
    pub fn transition_to_waiting_worker(
        &mut self,
        task_id: &str,
    ) -> Result<&ConductorTaskState, super::protocol::ConductorError> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| super::protocol::ConductorError::NotFound(task_id.to_string()))?;

        task.status = shared_types::ConductorTaskStatus::WaitingWorker;
        task.updated_at = chrono::Utc::now();

        Ok(&*task)
    }

    /// Transition a task to Completed status with report path
    pub fn transition_to_completed(
        &mut self,
        task_id: &str,
        output_mode: shared_types::ConductorOutputMode,
        report_path: String,
        toast: Option<ConductorToastPayload>,
    ) -> Result<&ConductorTaskState, super::protocol::ConductorError> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| super::protocol::ConductorError::NotFound(task_id.to_string()))?;

        task.status = shared_types::ConductorTaskStatus::Completed;
        task.output_mode = output_mode;
        task.report_path = Some(report_path);
        task.toast = toast;
        task.updated_at = chrono::Utc::now();
        task.completed_at = Some(chrono::Utc::now());

        Ok(&*task)
    }

    /// Transition a task to Failed status with error
    pub fn transition_to_failed(
        &mut self,
        task_id: &str,
        error: shared_types::ConductorError,
    ) -> Result<&ConductorTaskState, super::protocol::ConductorError> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| super::protocol::ConductorError::NotFound(task_id.to_string()))?;

        task.status = shared_types::ConductorTaskStatus::Failed;
        task.error = Some(error);
        task.updated_at = chrono::Utc::now();
        task.completed_at = Some(chrono::Utc::now());

        Ok(&*task)
    }

    /// Get all tasks (for debugging/inspection)
    pub fn get_all_tasks(&self) -> &HashMap<String, ConductorTaskState> {
        &self.tasks
    }

    /// Remove a task from state
    pub fn remove_task(&mut self, task_id: &str) -> Option<ConductorTaskState> {
        self.tasks.remove(task_id)
    }

    // =========================================================================
    // New Runtime Model: Run Management
    // =========================================================================

    /// Create or insert a run
    pub fn insert_run(&mut self, run: ConductorRunState) {
        self.runs.insert(run.run_id.clone(), run);
    }

    /// Get a run by ID
    pub fn get_run(&self, run_id: &str) -> Option<&ConductorRunState> {
        self.runs.get(run_id)
    }

    /// Get a mutable run by ID
    pub fn get_run_mut(&mut self, run_id: &str) -> Option<&mut ConductorRunState> {
        self.runs.get_mut(run_id)
    }

    /// Replace a run state entirely
    pub fn update_run(&mut self, run: ConductorRunState) {
        self.runs.insert(run.run_id.clone(), run);
    }

    /// Get all runs
    pub fn get_all_runs(&self) -> &HashMap<String, ConductorRunState> {
        &self.runs
    }

    /// Remove a run
    pub fn remove_run(&mut self, run_id: &str) -> Option<ConductorRunState> {
        // Also clean up active calls for this run
        self.active_calls.retain(|_, (rid, _)| rid != run_id);
        self.runs.remove(run_id)
    }

    // =========================================================================
    // Agenda Management
    // =========================================================================

    /// Add agenda items to a run
    pub fn add_agenda_items(
        &mut self,
        run_id: &str,
        items: Vec<ConductorAgendaItem>,
    ) -> Result<(), super::protocol::ConductorError> {
        let run = self
            .runs
            .get_mut(run_id)
            .ok_or_else(|| super::protocol::ConductorError::NotFound(run_id.to_string()))?;

        run.agenda.extend(items);
        run.updated_at = chrono::Utc::now();
        Ok(())
    }

    /// Update an agenda item's status
    pub fn update_agenda_item(
        &mut self,
        run_id: &str,
        item_id: &str,
        status: AgendaItemStatus,
    ) -> Result<(), super::protocol::ConductorError> {
        let run = self
            .runs
            .get_mut(run_id)
            .ok_or_else(|| super::protocol::ConductorError::NotFound(run_id.to_string()))?;

        let now = chrono::Utc::now();

        if let Some(item) = run.agenda.iter_mut().find(|i| i.item_id == item_id) {
            item.status = status;
            match status {
                AgendaItemStatus::Running => {
                    item.started_at = Some(now);
                }
                AgendaItemStatus::Completed
                | AgendaItemStatus::Failed
                | AgendaItemStatus::Blocked => {
                    item.completed_at = Some(now);
                }
                _ => {}
            }
            run.updated_at = now;
            Ok(())
        } else {
            Err(super::protocol::ConductorError::NotFound(format!(
                "agenda item {} in run {}",
                item_id, run_id
            )))
        }
    }

    /// Get agenda items that are ready to run (status == Ready)
    pub fn get_ready_agenda_items(&self, run_id: &str) -> Vec<&ConductorAgendaItem> {
        let Some(run) = self.runs.get(run_id) else {
            return Vec::new();
        };

        run.agenda
            .iter()
            .filter(|item| item.status == AgendaItemStatus::Ready)
            .collect()
    }

    /// Mark agenda items as ready when dependencies are satisfied
    pub fn update_agenda_item_readiness(
        &mut self,
        run_id: &str,
    ) -> Result<usize, super::protocol::ConductorError> {
        let run = self
            .runs
            .get_mut(run_id)
            .ok_or_else(|| super::protocol::ConductorError::NotFound(run_id.to_string()))?;

        let completed: std::collections::HashSet<_> = run
            .agenda
            .iter()
            .filter(|i| matches!(i.status, AgendaItemStatus::Completed))
            .map(|i| i.item_id.clone())
            .collect();

        let mut updated = 0;
        for item in run.agenda.iter_mut() {
            if item.status == AgendaItemStatus::Pending
                && item.depends_on.iter().all(|dep| completed.contains(dep))
            {
                item.status = AgendaItemStatus::Ready;
                updated += 1;
            }
        }

        if updated > 0 {
            run.updated_at = chrono::Utc::now();
        }

        Ok(updated)
    }

    // =========================================================================
    // Capability Call Management
    // =========================================================================

    /// Register a new capability call
    pub fn register_capability_call(
        &mut self,
        run_id: &str,
        call: ConductorCapabilityCall,
    ) -> Result<(), super::protocol::ConductorError> {
        let run = self
            .runs
            .get_mut(run_id)
            .ok_or_else(|| super::protocol::ConductorError::NotFound(run_id.to_string()))?;

        // Index for quick lookup
        self.active_calls
            .insert(call.call_id.clone(), (run_id.to_string(), call.clone()));

        // Add to run's active calls
        run.active_calls.push(call);
        run.updated_at = chrono::Utc::now();

        Ok(())
    }

    /// Update a capability call's status
    pub fn update_capability_call(
        &mut self,
        run_id: &str,
        call_id: &str,
        status: CapabilityCallStatus,
        error: Option<String>,
    ) -> Result<(), super::protocol::ConductorError> {
        let run = self
            .runs
            .get_mut(run_id)
            .ok_or_else(|| super::protocol::ConductorError::NotFound(run_id.to_string()))?;

        let now = chrono::Utc::now();

        if let Some(call) = run.active_calls.iter_mut().find(|c| c.call_id == call_id) {
            call.status = status;
            call.error = error.clone();

            if matches!(
                status,
                CapabilityCallStatus::Completed
                    | CapabilityCallStatus::Failed
                    | CapabilityCallStatus::Blocked
            ) {
                call.completed_at = Some(now);
                // Remove from active calls index when terminal
                self.active_calls.remove(call_id);
            } else {
                // Keep the index in sync for non-terminal transitions
                self.active_calls
                    .insert(call_id.to_string(), (run_id.to_string(), call.clone()));
            }

            run.updated_at = now;
            Ok(())
        } else {
            Err(super::protocol::ConductorError::NotFound(format!(
                "capability call {} in run {}",
                call_id, run_id
            )))
        }
    }

    /// Get a capability call by ID (from any run)
    pub fn get_capability_call(&self, call_id: &str) -> Option<&(String, ConductorCapabilityCall)> {
        self.active_calls.get(call_id)
    }

    /// Get active calls for a run
    pub fn get_run_active_calls(&self, run_id: &str) -> Vec<&ConductorCapabilityCall> {
        let Some(run) = self.runs.get(run_id) else {
            return Vec::new();
        };
        run.active_calls
            .iter()
            .filter(|c| {
                matches!(
                    c.status,
                    CapabilityCallStatus::Pending | CapabilityCallStatus::Running
                )
            })
            .collect()
    }

    // =========================================================================
    // Artifact Management
    // =========================================================================

    /// Add an artifact to a run
    pub fn add_artifact(
        &mut self,
        run_id: &str,
        artifact: ConductorArtifact,
    ) -> Result<(), super::protocol::ConductorError> {
        let run = self
            .runs
            .get_mut(run_id)
            .ok_or_else(|| super::protocol::ConductorError::NotFound(run_id.to_string()))?;

        // Link artifact to its source call
        if let Some(call) = run
            .active_calls
            .iter_mut()
            .find(|c| c.call_id == artifact.source_call_id)
        {
            call.artifact_ids.push(artifact.artifact_id.clone());
        }

        run.artifacts.push(artifact);
        run.updated_at = chrono::Utc::now();
        Ok(())
    }

    /// Get artifacts for a run
    pub fn get_artifacts(&self, run_id: &str) -> Vec<&ConductorArtifact> {
        let Some(run) = self.runs.get(run_id) else {
            return Vec::new();
        };
        run.artifacts.iter().collect()
    }

    /// Get artifacts produced by a specific call
    pub fn get_call_artifacts(&self, run_id: &str, call_id: &str) -> Vec<&ConductorArtifact> {
        let Some(run) = self.runs.get(run_id) else {
            return Vec::new();
        };
        run.artifacts
            .iter()
            .filter(|a| a.source_call_id == call_id)
            .collect()
    }

    // =========================================================================
    // Decision Log
    // =========================================================================

    /// Record a decision
    pub fn record_decision(
        &mut self,
        run_id: &str,
        decision: ConductorDecision,
    ) -> Result<(), super::protocol::ConductorError> {
        let run = self
            .runs
            .get_mut(run_id)
            .ok_or_else(|| super::protocol::ConductorError::NotFound(run_id.to_string()))?;

        run.decision_log.push(decision);
        run.updated_at = chrono::Utc::now();
        Ok(())
    }

    /// Get decision log for a run
    pub fn get_decisions(&self, run_id: &str) -> Vec<&ConductorDecision> {
        let Some(run) = self.runs.get(run_id) else {
            return Vec::new();
        };
        run.decision_log.iter().collect()
    }

    // =========================================================================
    // Run Lifecycle
    // =========================================================================

    /// Transition run status
    pub fn transition_run_status(
        &mut self,
        run_id: &str,
        status: ConductorRunStatus,
    ) -> Result<(), super::protocol::ConductorError> {
        let run = self
            .runs
            .get_mut(run_id)
            .ok_or_else(|| super::protocol::ConductorError::NotFound(run_id.to_string()))?;

        run.status = status;
        run.updated_at = chrono::Utc::now();

        if matches!(
            status,
            ConductorRunStatus::Completed
                | ConductorRunStatus::Failed
                | ConductorRunStatus::Blocked
        ) {
            run.completed_at = Some(chrono::Utc::now());
        }

        Ok(())
    }

    /// Check if a run has active work (active calls or ready agenda items)
    pub fn has_active_work(&self, run_id: &str) -> bool {
        let Some(run) = self.runs.get(run_id) else {
            return false;
        };

        let has_active_calls = run.active_calls.iter().any(|c| {
            matches!(
                c.status,
                CapabilityCallStatus::Pending | CapabilityCallStatus::Running
            )
        });

        let has_ready_items = run.agenda.iter().any(|i| {
            matches!(
                i.status,
                AgendaItemStatus::Ready | AgendaItemStatus::Running
            )
        });

        has_active_calls || has_ready_items
    }

    /// Get summary of run state for observability
    pub fn get_run_summary(&self, run_id: &str) -> Option<RunSummary> {
        let run = self.runs.get(run_id)?;

        Some(RunSummary {
            run_id: run.run_id.clone(),
            status: run.status,
            agenda_total: run.agenda.len(),
            agenda_pending: run
                .agenda
                .iter()
                .filter(|i| i.status == AgendaItemStatus::Pending)
                .count(),
            agenda_ready: run
                .agenda
                .iter()
                .filter(|i| i.status == AgendaItemStatus::Ready)
                .count(),
            agenda_running: run
                .agenda
                .iter()
                .filter(|i| i.status == AgendaItemStatus::Running)
                .count(),
            agenda_completed: run
                .agenda
                .iter()
                .filter(|i| i.status == AgendaItemStatus::Completed)
                .count(),
            agenda_failed: run
                .agenda
                .iter()
                .filter(|i| i.status == AgendaItemStatus::Failed)
                .count(),
            agenda_blocked: run
                .agenda
                .iter()
                .filter(|i| i.status == AgendaItemStatus::Blocked)
                .count(),
            active_calls: run.active_calls.len(),
            artifacts: run.artifacts.len(),
            decisions: run.decision_log.len(),
        })
    }
}

impl Default for ConductorState {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of a run for observability
#[derive(Debug, Clone)]
pub struct RunSummary {
    pub run_id: String,
    pub status: ConductorRunStatus,
    pub agenda_total: usize,
    pub agenda_pending: usize,
    pub agenda_ready: usize,
    pub agenda_running: usize,
    pub agenda_completed: usize,
    pub agenda_failed: usize,
    pub agenda_blocked: usize,
    pub active_calls: usize,
    pub artifacts: usize,
    pub decisions: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_types::{AgendaItemStatus, ConductorRunStatus};

    // ============================================================================
    // StartRun and Run State Tests
    // ============================================================================

    #[test]
    fn test_start_run_with_explicit_task_id() {
        let mut state = ConductorState::new();

        let run = ConductorRunState {
            run_id: "run_1".to_string(),
            task_id: "task_abc".to_string(), // Explicit task_id
            objective: "Test objective".to_string(),
            status: ConductorRunStatus::Running,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            completed_at: None,
            agenda: vec![],
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path: format!("conductor/runs/{}/draft.md", "run_1"),
            output_mode: shared_types::ConductorOutputMode::Auto,
            desktop_id: "desktop_1".to_string(),
            correlation_id: "corr_1".to_string(),
        };

        state.insert_run(run);

        let retrieved = state.get_run("run_1").unwrap();
        assert_eq!(retrieved.run_id, "run_1");
        assert_eq!(retrieved.task_id, "task_abc"); // Preserves explicit task_id
        assert_eq!(retrieved.status, ConductorRunStatus::Running);
    }

    #[test]
    fn test_start_run_without_task_id_uses_run_id() {
        let mut state = ConductorState::new();

        // Simulate StartRun without explicit task_id by using run_id as task_id
        let run = ConductorRunState {
            run_id: "run_only_id".to_string(),
            task_id: "run_only_id".to_string(), // Same as run_id (backward compatibility)
            objective: "Test objective".to_string(),
            status: ConductorRunStatus::Running,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            completed_at: None,
            agenda: vec![],
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path: format!("conductor/runs/{}/draft.md", "run_only_id"),
            output_mode: shared_types::ConductorOutputMode::Auto,
            desktop_id: "desktop_1".to_string(),
            correlation_id: "corr_1".to_string(),
        };

        state.insert_run(run);

        let retrieved = state.get_run("run_only_id").unwrap();
        assert_eq!(retrieved.run_id, "run_only_id");
        assert_eq!(retrieved.task_id, "run_only_id"); // task_id = run_id for backward compat
    }

    #[test]
    fn test_run_lifecycle() {
        let mut state = ConductorState::new();

        // Create a run
        let run = ConductorRunState {
            run_id: "run_1".to_string(),
            task_id: "task_1".to_string(),
            objective: "Test objective".to_string(),
            status: ConductorRunStatus::Initializing,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            completed_at: None,
            agenda: vec![],
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path: format!("conductor/runs/{}/draft.md", "run_1"),
            output_mode: shared_types::ConductorOutputMode::Auto,
            desktop_id: "desktop_1".to_string(),
            correlation_id: "corr_1".to_string(),
        };

        state.insert_run(run);
        assert!(state.get_run("run_1").is_some());

        // Transition to running
        state
            .transition_run_status("run_1", ConductorRunStatus::Running)
            .unwrap();
        assert_eq!(
            state.get_run("run_1").unwrap().status,
            ConductorRunStatus::Running
        );

        // Add agenda items
        let items = vec![
            ConductorAgendaItem {
                item_id: "item_1".to_string(),
                capability: "terminal".to_string(),
                objective: "Run command".to_string(),
                priority: 0,
                depends_on: vec![],
                status: AgendaItemStatus::Pending,
                created_at: chrono::Utc::now(),
                started_at: None,
                completed_at: None,
            },
            ConductorAgendaItem {
                item_id: "item_2".to_string(),
                capability: "researcher".to_string(),
                objective: "Research topic".to_string(),
                priority: 1,
                depends_on: vec!["item_1".to_string()],
                status: AgendaItemStatus::Pending,
                created_at: chrono::Utc::now(),
                started_at: None,
                completed_at: None,
            },
        ];

        state.add_agenda_items("run_1", items).unwrap();
        let run = state.get_run("run_1").unwrap();
        assert_eq!(run.agenda.len(), 2);

        // Update readiness
        let updated = state.update_agenda_item_readiness("run_1").unwrap();
        assert_eq!(updated, 1); // item_1 should be ready

        let ready = state.get_ready_agenda_items("run_1");
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].item_id, "item_1");

        // Complete item_1
        state
            .update_agenda_item("run_1", "item_1", AgendaItemStatus::Completed)
            .unwrap();

        // Now item_2 should be ready
        state.update_agenda_item_readiness("run_1").unwrap();
        let ready = state.get_ready_agenda_items("run_1");
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].item_id, "item_2");
    }

    #[test]
    fn test_run_status_transitions() {
        let mut state = ConductorState::new();

        let run = ConductorRunState {
            run_id: "run_status_test".to_string(),
            task_id: "task_1".to_string(),
            objective: "Test status transitions".to_string(),
            status: ConductorRunStatus::Initializing,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            completed_at: None,
            agenda: vec![],
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path: format!("conductor/runs/{}/draft.md", "run_status_test"),
            output_mode: shared_types::ConductorOutputMode::Auto,
            desktop_id: "desktop_1".to_string(),
            correlation_id: "corr_1".to_string(),
        };

        state.insert_run(run);

        // Initializing -> Running
        state
            .transition_run_status("run_status_test", ConductorRunStatus::Running)
            .unwrap();
        assert_eq!(
            state.get_run("run_status_test").unwrap().status,
            ConductorRunStatus::Running
        );
        assert!(state
            .get_run("run_status_test")
            .unwrap()
            .completed_at
            .is_none());

        // Running -> Completed (should set completed_at)
        state
            .transition_run_status("run_status_test", ConductorRunStatus::Completed)
            .unwrap();
        let run = state.get_run("run_status_test").unwrap();
        assert_eq!(run.status, ConductorRunStatus::Completed);
        assert!(run.completed_at.is_some());
    }

    #[test]
    fn test_run_status_failed_sets_completed_at() {
        let mut state = ConductorState::new();

        let run = ConductorRunState {
            run_id: "run_failed_test".to_string(),
            task_id: "task_1".to_string(),
            objective: "Test failure".to_string(),
            status: ConductorRunStatus::Running,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            completed_at: None,
            agenda: vec![],
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path: format!("conductor/runs/{}/draft.md", "run_failed_test"),
            output_mode: shared_types::ConductorOutputMode::Auto,
            desktop_id: "desktop_1".to_string(),
            correlation_id: "corr_1".to_string(),
        };

        state.insert_run(run);

        state
            .transition_run_status("run_failed_test", ConductorRunStatus::Failed)
            .unwrap();
        let run = state.get_run("run_failed_test").unwrap();
        assert_eq!(run.status, ConductorRunStatus::Failed);
        assert!(run.completed_at.is_some());
    }

    #[test]
    fn test_run_status_blocked_sets_completed_at() {
        let mut state = ConductorState::new();

        let run = ConductorRunState {
            run_id: "run_blocked_test".to_string(),
            task_id: "task_1".to_string(),
            objective: "Test blocked".to_string(),
            status: ConductorRunStatus::Running,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            completed_at: None,
            agenda: vec![],
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path: format!("conductor/runs/{}/draft.md", "run_blocked_test"),
            output_mode: shared_types::ConductorOutputMode::Auto,
            desktop_id: "desktop_1".to_string(),
            correlation_id: "corr_1".to_string(),
        };

        state.insert_run(run);

        state
            .transition_run_status("run_blocked_test", ConductorRunStatus::Blocked)
            .unwrap();
        let run = state.get_run("run_blocked_test").unwrap();
        assert_eq!(run.status, ConductorRunStatus::Blocked);
        assert!(run.completed_at.is_some());
    }

    // ============================================================================
    // Agenda Item Management Tests
    // ============================================================================

    #[test]
    fn test_get_ready_agenda_items_empty_run() {
        let state = ConductorState::new();

        // Should return empty vec for non-existent run
        let ready = state.get_ready_agenda_items("non_existent_run");
        assert!(ready.is_empty());
    }

    #[test]
    fn test_get_ready_agenda_items_no_ready_items() {
        let mut state = ConductorState::new();

        let run = ConductorRunState {
            run_id: "run_no_ready".to_string(),
            task_id: "task_1".to_string(),
            objective: "Test".to_string(),
            status: ConductorRunStatus::Running,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            completed_at: None,
            agenda: vec![ConductorAgendaItem {
                item_id: "item_1".to_string(),
                capability: "terminal".to_string(),
                objective: "Run command".to_string(),
                priority: 0,
                depends_on: vec!["missing_dep".to_string()], // Unsatisfied dependency
                status: AgendaItemStatus::Pending,
                created_at: chrono::Utc::now(),
                started_at: None,
                completed_at: None,
            }],
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path: format!("conductor/runs/{}/draft.md", "run_no_ready"),
            output_mode: shared_types::ConductorOutputMode::Auto,
            desktop_id: "desktop_1".to_string(),
            correlation_id: "corr_1".to_string(),
        };

        state.insert_run(run);

        let ready = state.get_ready_agenda_items("run_no_ready");
        assert!(ready.is_empty());
    }

    #[test]
    fn test_dispatch_ready_scenario_multiple_items() {
        let mut state = ConductorState::new();

        // Setup: 3 items - 1 completed, 1 ready (no deps), 1 pending (depends on ready)
        let items = vec![
            ConductorAgendaItem {
                item_id: "item_completed".to_string(),
                capability: "terminal".to_string(),
                objective: "Done".to_string(),
                priority: 0,
                depends_on: vec![],
                status: AgendaItemStatus::Completed,
                created_at: chrono::Utc::now(),
                started_at: Some(chrono::Utc::now()),
                completed_at: Some(chrono::Utc::now()),
            },
            ConductorAgendaItem {
                item_id: "item_ready".to_string(),
                capability: "researcher".to_string(),
                objective: "Ready to run".to_string(),
                priority: 1,
                depends_on: vec![], // No deps, should be ready
                status: AgendaItemStatus::Pending,
                created_at: chrono::Utc::now(),
                started_at: None,
                completed_at: None,
            },
            ConductorAgendaItem {
                item_id: "item_blocked".to_string(),
                capability: "writer".to_string(),
                objective: "Blocked on ready".to_string(),
                priority: 2,
                depends_on: vec!["item_ready".to_string()],
                status: AgendaItemStatus::Pending,
                created_at: chrono::Utc::now(),
                started_at: None,
                completed_at: None,
            },
        ];

        let run = ConductorRunState {
            run_id: "run_dispatch".to_string(),
            task_id: "task_1".to_string(),
            objective: "Test dispatch".to_string(),
            status: ConductorRunStatus::Running,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            completed_at: None,
            agenda: items,
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path: format!("conductor/runs/{}/draft.md", "run_dispatch"),
            output_mode: shared_types::ConductorOutputMode::Auto,
            desktop_id: "desktop_1".to_string(),
            correlation_id: "corr_1".to_string(),
        };

        state.insert_run(run);

        // Before update_agenda_item_readiness, item_ready is still Pending
        let ready = state.get_ready_agenda_items("run_dispatch");
        assert_eq!(ready.len(), 0); // item_ready is still Pending

        // After update_agenda_item_readiness, item_ready should become Ready
        let updated = state.update_agenda_item_readiness("run_dispatch").unwrap();
        assert_eq!(updated, 1);

        let ready = state.get_ready_agenda_items("run_dispatch");
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].item_id, "item_ready");

        // Simulate dispatch by marking item_ready as Running
        state
            .update_agenda_item("run_dispatch", "item_ready", AgendaItemStatus::Running)
            .unwrap();

        // Now no items should be ready (item_blocked still depends on item_ready completing)
        let ready = state.get_ready_agenda_items("run_dispatch");
        assert_eq!(ready.len(), 0);

        // Complete item_ready
        state
            .update_agenda_item("run_dispatch", "item_ready", AgendaItemStatus::Completed)
            .unwrap();

        // Now item_blocked should become ready
        let updated = state.update_agenda_item_readiness("run_dispatch").unwrap();
        assert_eq!(updated, 1);

        let ready = state.get_ready_agenda_items("run_dispatch");
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].item_id, "item_blocked");
    }

    #[test]
    fn test_agenda_item_status_transitions() {
        let mut state = ConductorState::new();

        let items = vec![ConductorAgendaItem {
            item_id: "item_1".to_string(),
            capability: "terminal".to_string(),
            objective: "Test transitions".to_string(),
            priority: 0,
            depends_on: vec![],
            status: AgendaItemStatus::Pending,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
        }];

        let run = ConductorRunState {
            run_id: "run_transitions".to_string(),
            task_id: "task_1".to_string(),
            objective: "Test".to_string(),
            status: ConductorRunStatus::Running,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            completed_at: None,
            agenda: items,
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path: format!("conductor/runs/{}/draft.md", "run_transitions"),
            output_mode: shared_types::ConductorOutputMode::Auto,
            desktop_id: "desktop_1".to_string(),
            correlation_id: "corr_1".to_string(),
        };

        state.insert_run(run);

        // Pending -> Ready
        state
            .update_agenda_item("run_transitions", "item_1", AgendaItemStatus::Ready)
            .unwrap();
        let item = state
            .get_run("run_transitions")
            .unwrap()
            .agenda
            .iter()
            .find(|i| i.item_id == "item_1")
            .unwrap();
        assert_eq!(item.status, AgendaItemStatus::Ready);
        assert!(item.started_at.is_none()); // started_at only set on Running

        // Ready -> Running
        state
            .update_agenda_item("run_transitions", "item_1", AgendaItemStatus::Running)
            .unwrap();
        let item = state
            .get_run("run_transitions")
            .unwrap()
            .agenda
            .iter()
            .find(|i| i.item_id == "item_1")
            .unwrap();
        assert_eq!(item.status, AgendaItemStatus::Running);
        assert!(item.started_at.is_some());

        // Running -> Completed
        state
            .update_agenda_item("run_transitions", "item_1", AgendaItemStatus::Completed)
            .unwrap();
        let item = state
            .get_run("run_transitions")
            .unwrap()
            .agenda
            .iter()
            .find(|i| i.item_id == "item_1")
            .unwrap();
        assert_eq!(item.status, AgendaItemStatus::Completed);
        assert!(item.completed_at.is_some());
    }

    #[test]
    fn test_agenda_item_failed_status() {
        let mut state = ConductorState::new();

        let items = vec![ConductorAgendaItem {
            item_id: "item_1".to_string(),
            capability: "terminal".to_string(),
            objective: "Will fail".to_string(),
            priority: 0,
            depends_on: vec![],
            status: AgendaItemStatus::Running,
            created_at: chrono::Utc::now(),
            started_at: Some(chrono::Utc::now()),
            completed_at: None,
        }];

        let run = ConductorRunState {
            run_id: "run_failed".to_string(),
            task_id: "task_1".to_string(),
            objective: "Test".to_string(),
            status: ConductorRunStatus::Running,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            completed_at: None,
            agenda: items,
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path: format!("conductor/runs/{}/draft.md", "run_failed"),
            output_mode: shared_types::ConductorOutputMode::Auto,
            desktop_id: "desktop_1".to_string(),
            correlation_id: "corr_1".to_string(),
        };

        state.insert_run(run);

        // Running -> Failed
        state
            .update_agenda_item("run_failed", "item_1", AgendaItemStatus::Failed)
            .unwrap();
        let item = state
            .get_run("run_failed")
            .unwrap()
            .agenda
            .iter()
            .find(|i| i.item_id == "item_1")
            .unwrap();
        assert_eq!(item.status, AgendaItemStatus::Failed);
        assert!(item.completed_at.is_some());
    }

    #[test]
    fn test_agenda_item_blocked_status() {
        let mut state = ConductorState::new();

        let items = vec![ConductorAgendaItem {
            item_id: "item_1".to_string(),
            capability: "terminal".to_string(),
            objective: "Will block".to_string(),
            priority: 0,
            depends_on: vec![],
            status: AgendaItemStatus::Running,
            created_at: chrono::Utc::now(),
            started_at: Some(chrono::Utc::now()),
            completed_at: None,
        }];

        let run = ConductorRunState {
            run_id: "run_blocked".to_string(),
            task_id: "task_1".to_string(),
            objective: "Test".to_string(),
            status: ConductorRunStatus::Running,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            completed_at: None,
            agenda: items,
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path: format!("conductor/runs/{}/draft.md", "run_blocked"),
            output_mode: shared_types::ConductorOutputMode::Auto,
            desktop_id: "desktop_1".to_string(),
            correlation_id: "corr_1".to_string(),
        };

        state.insert_run(run);

        // Running -> Blocked
        state
            .update_agenda_item("run_blocked", "item_1", AgendaItemStatus::Blocked)
            .unwrap();
        let item = state
            .get_run("run_blocked")
            .unwrap()
            .agenda
            .iter()
            .find(|i| i.item_id == "item_1")
            .unwrap();
        assert_eq!(item.status, AgendaItemStatus::Blocked);
        assert!(item.completed_at.is_some());
    }

    #[test]
    fn test_update_agenda_item_nonexistent_run() {
        let mut state = ConductorState::new();

        let result = state.update_agenda_item("nonexistent", "item_1", AgendaItemStatus::Running);
        assert!(result.is_err());
        match result.unwrap_err() {
            super::super::protocol::ConductorError::NotFound(msg) => {
                assert!(msg.contains("nonexistent"));
            }
            other => panic!("Expected NotFound, got {:?}", other),
        }
    }

    #[test]
    fn test_update_agenda_item_nonexistent_item() {
        let mut state = ConductorState::new();

        let run = ConductorRunState {
            run_id: "run_exists".to_string(),
            task_id: "task_1".to_string(),
            objective: "Test".to_string(),
            status: ConductorRunStatus::Running,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            completed_at: None,
            agenda: vec![],
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path: format!("conductor/runs/{}/draft.md", "run_exists"),
            output_mode: shared_types::ConductorOutputMode::Auto,
            desktop_id: "desktop_1".to_string(),
            correlation_id: "corr_1".to_string(),
        };

        state.insert_run(run);

        let result =
            state.update_agenda_item("run_exists", "nonexistent_item", AgendaItemStatus::Running);
        assert!(result.is_err());
        match result.unwrap_err() {
            super::super::protocol::ConductorError::NotFound(msg) => {
                assert!(msg.contains("nonexistent_item"));
            }
            other => panic!("Expected NotFound, got {:?}", other),
        }
    }

    #[test]
    fn test_has_active_work_with_active_calls() {
        let mut state = ConductorState::new();

        let run = ConductorRunState {
            run_id: "run_active".to_string(),
            task_id: "task_1".to_string(),
            objective: "Test".to_string(),
            status: ConductorRunStatus::Running,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            completed_at: None,
            agenda: vec![],
            active_calls: vec![ConductorCapabilityCall {
                call_id: "call_1".to_string(),
                capability: "terminal".to_string(),
                objective: "Running".to_string(),
                status: shared_types::CapabilityCallStatus::Running,
                started_at: chrono::Utc::now(),
                completed_at: None,
                parent_call_id: None,
                agenda_item_id: None,
                artifact_ids: vec![],
                error: None,
            }],
            artifacts: vec![],
            decision_log: vec![],
            document_path: format!("conductor/runs/{}/draft.md", "run_active"),
            output_mode: shared_types::ConductorOutputMode::Auto,
            desktop_id: "desktop_1".to_string(),
            correlation_id: "corr_1".to_string(),
        };

        state.insert_run(run);

        assert!(state.has_active_work("run_active"));
    }

    #[test]
    fn test_has_active_work_with_ready_items() {
        let mut state = ConductorState::new();

        let run = ConductorRunState {
            run_id: "run_ready".to_string(),
            task_id: "task_1".to_string(),
            objective: "Test".to_string(),
            status: ConductorRunStatus::Running,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            completed_at: None,
            agenda: vec![ConductorAgendaItem {
                item_id: "item_1".to_string(),
                capability: "terminal".to_string(),
                objective: "Ready".to_string(),
                priority: 0,
                depends_on: vec![],
                status: AgendaItemStatus::Ready,
                created_at: chrono::Utc::now(),
                started_at: None,
                completed_at: None,
            }],
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path: format!("conductor/runs/{}/draft.md", "run_ready"),
            output_mode: shared_types::ConductorOutputMode::Auto,
            desktop_id: "desktop_1".to_string(),
            correlation_id: "corr_1".to_string(),
        };

        state.insert_run(run);

        assert!(state.has_active_work("run_ready"));
    }

    #[test]
    fn test_has_active_work_no_active_work() {
        let mut state = ConductorState::new();

        let run = ConductorRunState {
            run_id: "run_idle".to_string(),
            task_id: "task_1".to_string(),
            objective: "Test".to_string(),
            status: ConductorRunStatus::Running,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            completed_at: None,
            agenda: vec![ConductorAgendaItem {
                item_id: "item_1".to_string(),
                capability: "terminal".to_string(),
                objective: "Pending".to_string(),
                priority: 0,
                depends_on: vec!["unmet_dep".to_string()],
                status: AgendaItemStatus::Pending,
                created_at: chrono::Utc::now(),
                started_at: None,
                completed_at: None,
            }],
            active_calls: vec![ConductorCapabilityCall {
                call_id: "call_1".to_string(),
                capability: "terminal".to_string(),
                objective: "Completed".to_string(),
                status: shared_types::CapabilityCallStatus::Completed,
                started_at: chrono::Utc::now(),
                completed_at: Some(chrono::Utc::now()),
                parent_call_id: None,
                agenda_item_id: None,
                artifact_ids: vec![],
                error: None,
            }],
            artifacts: vec![],
            decision_log: vec![],
            document_path: format!("conductor/runs/{}/draft.md", "run_idle"),
            output_mode: shared_types::ConductorOutputMode::Auto,
            desktop_id: "desktop_1".to_string(),
            correlation_id: "corr_1".to_string(),
        };

        state.insert_run(run);

        assert!(!state.has_active_work("run_idle"));
    }

    #[test]
    fn test_get_run_summary() {
        let mut state = ConductorState::new();

        let items = vec![
            ConductorAgendaItem {
                item_id: "item_1".to_string(),
                capability: "terminal".to_string(),
                objective: "Pending".to_string(),
                priority: 0,
                depends_on: vec![],
                status: AgendaItemStatus::Pending,
                created_at: chrono::Utc::now(),
                started_at: None,
                completed_at: None,
            },
            ConductorAgendaItem {
                item_id: "item_2".to_string(),
                capability: "researcher".to_string(),
                objective: "Ready".to_string(),
                priority: 1,
                depends_on: vec![],
                status: AgendaItemStatus::Ready,
                created_at: chrono::Utc::now(),
                started_at: None,
                completed_at: None,
            },
            ConductorAgendaItem {
                item_id: "item_3".to_string(),
                capability: "writer".to_string(),
                objective: "Running".to_string(),
                priority: 2,
                depends_on: vec![],
                status: AgendaItemStatus::Running,
                created_at: chrono::Utc::now(),
                started_at: None,
                completed_at: None,
            },
            ConductorAgendaItem {
                item_id: "item_4".to_string(),
                capability: "terminal".to_string(),
                objective: "Completed".to_string(),
                priority: 3,
                depends_on: vec![],
                status: AgendaItemStatus::Completed,
                created_at: chrono::Utc::now(),
                started_at: None,
                completed_at: None,
            },
            ConductorAgendaItem {
                item_id: "item_5".to_string(),
                capability: "terminal".to_string(),
                objective: "Failed".to_string(),
                priority: 4,
                depends_on: vec![],
                status: AgendaItemStatus::Failed,
                created_at: chrono::Utc::now(),
                started_at: None,
                completed_at: None,
            },
            ConductorAgendaItem {
                item_id: "item_6".to_string(),
                capability: "terminal".to_string(),
                objective: "Blocked".to_string(),
                priority: 5,
                depends_on: vec![],
                status: AgendaItemStatus::Blocked,
                created_at: chrono::Utc::now(),
                started_at: None,
                completed_at: None,
            },
        ];

        let run = ConductorRunState {
            run_id: "run_summary".to_string(),
            task_id: "task_1".to_string(),
            objective: "Test".to_string(),
            status: ConductorRunStatus::Running,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            completed_at: None,
            agenda: items,
            active_calls: vec![ConductorCapabilityCall {
                call_id: "call_1".to_string(),
                capability: "terminal".to_string(),
                objective: "Active".to_string(),
                status: shared_types::CapabilityCallStatus::Running,
                started_at: chrono::Utc::now(),
                completed_at: None,
                parent_call_id: None,
                agenda_item_id: None,
                artifact_ids: vec![],
                error: None,
            }],
            artifacts: vec![ConductorArtifact {
                artifact_id: "art_1".to_string(),
                kind: shared_types::ArtifactKind::Report,
                source_call_id: "call_1".to_string(),
                reference: "/path/to/report.md".to_string(),
                mime_type: Some("text/markdown".to_string()),
                created_at: chrono::Utc::now(),
                metadata: None,
            }],
            decision_log: vec![ConductorDecision {
                decision_id: "dec_1".to_string(),
                decision_type: shared_types::DecisionType::Dispatch,
                reason: "Selected terminal".to_string(),
                timestamp: chrono::Utc::now(),
                affected_agenda_items: vec!["item_1".to_string()],
                new_agenda_items: vec![],
            }],
            document_path: format!("conductor/runs/{}/draft.md", "run_summary"),
            output_mode: shared_types::ConductorOutputMode::Auto,
            desktop_id: "desktop_1".to_string(),
            correlation_id: "corr_1".to_string(),
        };

        state.insert_run(run);

        let summary = state.get_run_summary("run_summary").unwrap();
        assert_eq!(summary.run_id, "run_summary");
        assert_eq!(summary.status, ConductorRunStatus::Running);
        assert_eq!(summary.agenda_total, 6);
        assert_eq!(summary.agenda_pending, 1);
        assert_eq!(summary.agenda_ready, 1);
        assert_eq!(summary.agenda_running, 1);
        assert_eq!(summary.agenda_completed, 1);
        assert_eq!(summary.agenda_failed, 1);
        assert_eq!(summary.agenda_blocked, 1);
        assert_eq!(summary.active_calls, 1);
        assert_eq!(summary.artifacts, 1);
        assert_eq!(summary.decisions, 1);
    }

    #[test]
    fn test_capability_call_tracking() {
        let mut state = ConductorState::new();

        let run = ConductorRunState {
            run_id: "run_1".to_string(),
            task_id: "task_1".to_string(),
            objective: "Test".to_string(),
            status: ConductorRunStatus::Running,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            completed_at: None,
            agenda: vec![],
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path: format!("conductor/runs/{}/draft.md", "run_1"),
            output_mode: shared_types::ConductorOutputMode::Auto,
            desktop_id: "desktop_1".to_string(),
            correlation_id: "corr_1".to_string(),
        };

        state.insert_run(run);

        let call = ConductorCapabilityCall {
            call_id: "call_1".to_string(),
            capability: "terminal".to_string(),
            objective: "Run ls".to_string(),
            status: CapabilityCallStatus::Running,
            started_at: chrono::Utc::now(),
            completed_at: None,
            parent_call_id: None,
            agenda_item_id: None,
            artifact_ids: vec![],
            error: None,
        };

        state.register_capability_call("run_1", call).unwrap();
        assert_eq!(state.get_run_active_calls("run_1").len(), 1);

        // Complete the call
        state
            .update_capability_call("run_1", "call_1", CapabilityCallStatus::Completed, None)
            .unwrap();
        assert_eq!(state.get_run_active_calls("run_1").len(), 0);
    }
}
