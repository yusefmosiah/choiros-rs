//! ConductorActor state management
//!
//! Manages the lifecycle of tasks and their state transitions.

use shared_types::{
    ConductorOutputMode, ConductorTaskState, ConductorTaskStatus, ConductorToastPayload,
};
use std::collections::HashMap;

/// State container for ConductorActor
pub struct ConductorState {
    /// Map of task_id to task state
    tasks: HashMap<String, ConductorTaskState>,
}

impl ConductorState {
    /// Create a new empty state
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
        }
    }

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

        task.status = ConductorTaskStatus::Running;
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

        task.status = ConductorTaskStatus::WaitingWorker;
        task.updated_at = chrono::Utc::now();

        Ok(&*task)
    }

    /// Transition a task to Completed status with report path
    pub fn transition_to_completed(
        &mut self,
        task_id: &str,
        output_mode: ConductorOutputMode,
        report_path: String,
        toast: Option<ConductorToastPayload>,
    ) -> Result<&ConductorTaskState, super::protocol::ConductorError> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| super::protocol::ConductorError::NotFound(task_id.to_string()))?;

        task.status = ConductorTaskStatus::Completed;
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

        task.status = ConductorTaskStatus::Failed;
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
}

impl Default for ConductorState {
    fn default() -> Self {
        Self::new()
    }
}
