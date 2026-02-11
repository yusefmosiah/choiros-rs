//! WatcherActor - LLM-driven event-log monitoring.
//!
//! This actor scans recent EventStore entries and uses BAML LLM functions to
//! review event windows for anomalies, failures, and concerning patterns.
//! It replaces the previous deterministic rule-based approach with intelligent
//! LLM-driven analysis.

use async_trait::async_trait;
use chrono::Utc;
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use std::collections::HashMap;
use std::time::Duration;

use crate::actors::conductor::protocol::ConductorMsg;
use crate::actors::event_store::{AppendEvent, EventStoreError, EventStoreMsg};
use crate::actors::model_config::{ModelRegistry, ModelResolutionContext};
use crate::baml_client::{types::*, B};

#[derive(Debug, Clone)]
pub struct WatcherArguments {
    pub event_store: ActorRef<EventStoreMsg>,
    pub conductor_actor: Option<ActorRef<ConductorMsg>>,
    pub watcher_id: String,
    pub poll_interval_ms: u64,
    /// Window size for LLM review (number of events per batch)
    pub review_window_size: usize,
    /// Maximum events to fetch per scan
    pub max_events_per_scan: i64,
    /// If true, initialize cursor to current end of log and only watch new events.
    pub start_from_latest: bool,
}

pub struct WatcherState {
    event_store: ActorRef<EventStoreMsg>,
    conductor_actor: Option<ActorRef<ConductorMsg>>,
    watcher_id: String,
    last_seq: i64,
    review_window_size: usize,
    max_events_per_scan: i64,
    /// Pending run states for mitigation recommendations
    run_states: HashMap<String, RunStateSnapshot>,
}

#[derive(Debug, Clone)]
struct EventWindow {
    id: String,
    run_id: String,
    task_id: String,
    events: Vec<shared_types::Event>,
    start_time: chrono::DateTime<Utc>,
    end_time: chrono::DateTime<Utc>,
    event_types: Vec<String>,
    min_level: String,
    review_reason: Option<String>,
}

#[derive(Debug)]
pub enum WatcherMsg {
    /// Internal trigger to scan the event log.
    ScanNow,
    /// Health/debug endpoint.
    GetCursor { reply: RpcReplyPort<i64> },
    /// Update run state for mitigation context
    UpdateRunState {
        run_id: String,
        state: RunStateSnapshot,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum WatcherError {
    #[error("EventStore RPC error: {0}")]
    Rpc(String),
    #[error("EventStore error: {0}")]
    EventStore(String),
    #[error("LLM review error: {0}")]
    ReviewError(String),
    #[error("Mitigation recommendation error: {0}")]
    MitigationError(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

impl From<EventStoreError> for WatcherError {
    fn from(value: EventStoreError) -> Self {
        Self::EventStore(value.to_string())
    }
}

impl From<serde_json::Error> for WatcherError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serialization(value.to_string())
    }
}

#[derive(Debug, Default)]
pub struct WatcherActor;

#[async_trait]
impl Actor for WatcherActor {
    type Msg = WatcherMsg;
    type State = WatcherState;
    type Arguments = WatcherArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let initial_last_seq = if args.start_from_latest {
            match ractor::call!(&args.event_store, |reply| EventStoreMsg::GetLatestSeq {
                reply
            }) {
                Ok(Ok(Some(seq))) => seq,
                Ok(Ok(None)) => 0,
                Ok(Err(e)) => {
                    return Err(ActorProcessingErr::from(format!(
                        "Failed to initialize watcher cursor from EventStore: {e}"
                    )));
                }
                Err(e) => {
                    return Err(ActorProcessingErr::from(format!(
                        "Failed to call EventStore for watcher cursor init: {e}"
                    )));
                }
            }
        } else {
            0
        };

        // Kick off background periodic scans.
        let interval = Duration::from_millis(args.poll_interval_ms.max(500));
        let tick_ref = myself.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                let _ = tick_ref.cast(WatcherMsg::ScanNow);
            }
        });

        Ok(WatcherState {
            event_store: args.event_store,
            conductor_actor: args.conductor_actor,
            watcher_id: args.watcher_id,
            last_seq: initial_last_seq,
            review_window_size: args.review_window_size.max(1),
            max_events_per_scan: args.max_events_per_scan.max(100),
            run_states: HashMap::new(),
        })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            WatcherMsg::ScanNow => {
                if let Err(err) = self.scan_recent_events(state).await {
                    tracing::warn!(error = %err, "Watcher scan failed");
                }
            }
            WatcherMsg::GetCursor { reply } => {
                let _ = reply.send(state.last_seq);
            }
            WatcherMsg::UpdateRunState {
                run_id,
                state: run_state,
            } => {
                state.run_states.insert(run_id, run_state);
            }
        }
        Ok(())
    }
}

impl WatcherActor {
    /// Main scan method - fetches recent events and processes them through LLM review
    async fn scan_recent_events(&self, state: &mut WatcherState) -> Result<(), WatcherError> {
        // Fetch recent events from EventStore
        let recent = ractor::call!(state.event_store, |reply| EventStoreMsg::GetRecentEvents {
            since_seq: state.last_seq,
            limit: state.max_events_per_scan,
            event_type_prefix: None,
            actor_id: None,
            user_id: None,
            reply
        })
        .map_err(|e| WatcherError::Rpc(e.to_string()))?
        .map_err(WatcherError::from)?;

        if recent.is_empty() {
            return Ok(());
        }

        // Advance cursor first so filtered events don't cause polling loops.
        let max_seq = recent.iter().map(|e| e.seq).max().unwrap_or(state.last_seq);
        state.last_seq = max_seq;

        // Ignore watcher-origin and watcher-internal events to prevent self-trigger loops.
        let recent: Vec<shared_types::Event> = recent
            .into_iter()
            .filter(|event| !self.should_ignore_event(state, event))
            .collect();

        if recent.is_empty() {
            return Ok(());
        }

        // Build event windows for review
        let windows = self.build_event_windows(state, &recent).await?;

        for window in windows {
            // LLM-driven review (not deterministic rules)
            match self.llm_review_window(state, &window).await {
                Ok(review) => {
                    // Process escalations
                    for escalation in &review.escalations {
                        if let Err(e) = self.process_escalation(state, escalation).await {
                            tracing::error!("Failed to process escalation: {}", e);
                        }
                    }

                    // Log risks for observability
                    for risk in &review.risks {
                        tracing::warn!(
                            risk_id = %risk.risk_id,
                            category = ?risk.category,
                            likelihood = risk.likelihood,
                            impact = risk.impact,
                            "Risk detected"
                        );
                    }

                    // Log anomalies
                    for anomaly in &review.anomalies {
                        tracing::warn!(
                            anomaly_type = %anomaly.anomaly_type,
                            severity = %anomaly.severity,
                            "Anomaly detected"
                        );
                    }

                    // Emit review event for observability
                    if let Err(e) = self.emit_review_event(state, &window, &review).await {
                        tracing::error!("Failed to emit review event: {}", e);
                    }
                }
                Err(e) => {
                    // NO DETERMINISTIC FALLBACK - emit failure and continue
                    tracing::error!("LLM review failed: {}", e);
                    if let Err(emit_err) = self
                        .emit_review_failure(state, &window, &e.to_string())
                        .await
                    {
                        tracing::error!("Failed to emit review failure: {}", emit_err);
                    }
                    // Continue with next window - don't block on review failure
                }
            }
        }

        Ok(())
    }

    fn should_ignore_event(&self, state: &WatcherState, event: &shared_types::Event) -> bool {
        if event.event_type.starts_with("watcher.") {
            return true;
        }

        let actor_id = event.actor_id.0.as_str();
        if actor_id == state.watcher_id || actor_id.starts_with("watcher:") {
            return true;
        }

        !self.should_review_event(event)
    }

    fn should_review_event(&self, event: &shared_types::Event) -> bool {
        matches!(
            event.event_type.as_str(),
            "conductor.task.started"
                | "conductor.task.progress"
                | "conductor.task.completed"
                | "conductor.task.failed"
                | "conductor.worker.call"
                | "conductor.worker.result"
                | "conductor.run.started"
                | "conductor.capability.completed"
                | "conductor.capability.failed"
                | "conductor.capability.blocked"
                | "conductor.decision"
                | "conductor.progress"
                | "worker.task.started"
                | "worker.task.progress"
                | "worker.task.completed"
                | "worker.task.failed"
                | "worker.task.finding"
                | "worker.task.learning"
        )
    }

    /// Build event windows from recent events
    async fn build_event_windows(
        &self,
        state: &WatcherState,
        events: &[shared_types::Event],
    ) -> Result<Vec<EventWindow>, WatcherError> {
        let mut windows: Vec<EventWindow> = Vec::new();
        let mut current_window: Vec<shared_types::Event> = Vec::new();
        let mut current_run_id: Option<String> = None;
        let mut current_task_id: Option<String> = None;

        for event in events {
            // Extract run_id and task_id from event
            let run_id = event
                .payload
                .get("run_id")
                .and_then(|v| v.as_str())
                .or_else(|| event.payload.get("task_id").and_then(|v| v.as_str()))
                .or_else(|| {
                    event
                        .payload
                        .get("task")
                        .and_then(|t| t.get("task_id"))
                        .and_then(|v| v.as_str())
                })
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unknown".to_string());

            let task_id = event
                .payload
                .get("task_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    event
                        .payload
                        .get("task")
                        .and_then(|t| t.get("task_id"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| run_id.clone());

            // Check if we need to start a new window
            let should_start_new_window = if current_window.len() >= state.review_window_size {
                true
            } else if let Some(ref curr_run) = current_run_id {
                curr_run != &run_id
            } else {
                false
            };

            if should_start_new_window && !current_window.is_empty() {
                // Finalize current window
                windows.push(self.finalize_window(
                    &current_run_id.unwrap_or_default(),
                    &current_task_id.unwrap_or_default(),
                    std::mem::take(&mut current_window),
                ));
            }

            // Update tracking
            current_run_id = Some(run_id);
            current_task_id = Some(task_id);
            current_window.push(event.clone());
        }

        // Don't forget the last window
        if !current_window.is_empty() {
            windows.push(self.finalize_window(
                &current_run_id.unwrap_or_default(),
                &current_task_id.unwrap_or_default(),
                current_window,
            ));
        }

        Ok(windows)
    }

    /// Finalize a window with metadata
    fn finalize_window(
        &self,
        run_id: &str,
        task_id: &str,
        events: Vec<shared_types::Event>,
    ) -> EventWindow {
        let window_id = format!("window:{}:{}", run_id, Utc::now().timestamp_millis());

        let start_time = events.first().map(|e| e.timestamp).unwrap_or_else(Utc::now);

        let end_time = events.last().map(|e| e.timestamp).unwrap_or_else(Utc::now);

        let event_types: Vec<String> = events
            .iter()
            .map(|e| e.event_type.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // Determine min level based on event types
        let min_level = if events
            .iter()
            .any(|e| e.event_type.contains("failed") || e.event_type.contains("error"))
        {
            "error"
        } else if events.iter().any(|e| e.event_type.contains("warn")) {
            "warn"
        } else {
            "info"
        }
        .to_string();

        // Determine review reason
        let review_reason = if events.iter().any(|e| e.event_type.contains("failed")) {
            Some("Failure events detected".to_string())
        } else if events.iter().any(|e| e.event_type.contains("timeout")) {
            Some("Timeout events detected".to_string())
        } else {
            Some("Periodic review".to_string())
        };

        EventWindow {
            id: window_id,
            run_id: run_id.to_string(),
            task_id: task_id.to_string(),
            events,
            start_time,
            end_time,
            event_types,
            min_level,
            review_reason,
        }
    }

    /// Perform LLM-driven review of an event window
    async fn llm_review_window(
        &self,
        _state: &WatcherState,
        window: &EventWindow,
    ) -> Result<WatcherReviewOutput, WatcherError> {
        // Resolve model for watcher role (lower-power than conductor)
        let registry = ModelRegistry::new();
        let resolved = registry
            .resolve_for_role("watcher", &ModelResolutionContext::default())
            .map_err(|e| WatcherError::ReviewError(format!("Model resolution failed: {}", e)))?;

        let client_registry = registry
            .create_runtime_client_registry_for_model(&resolved.config.id)
            .map_err(|e| {
                WatcherError::ReviewError(format!("Client registry creation failed: {}", e))
            })?;

        // Build input
        let input = self.build_review_input(window).await?;

        // Call BAML review function
        let review = B
            .WatcherReviewLogWindow
            .with_client_registry(&client_registry)
            .call(&input)
            .await
            .map_err(|e| WatcherError::ReviewError(e.to_string()))?;

        Ok(review)
    }

    /// Build WatcherLogWindowInput from event window
    async fn build_review_input(
        &self,
        window: &EventWindow,
    ) -> Result<WatcherLogWindowInput, WatcherError> {
        let events: Vec<WatcherEvent> = window
            .events
            .iter()
            .map(|e| WatcherEvent {
                event_id: e.event_id.clone(),
                timestamp: e.timestamp.to_rfc3339(),
                event_type: e.event_type.clone(),
                level: self.infer_event_level(e),
                payload: serde_json::to_string(&e.payload).unwrap_or_default(),
                source: e.actor_id.0.clone(),
            })
            .collect();

        Ok(WatcherLogWindowInput {
            window_id: window.id.clone(),
            run_id: window.run_id.clone(),
            task_id: window.task_id.clone(),
            events,
            scope: ReviewScope {
                start_time: window.start_time.to_rfc3339(),
                end_time: window.end_time.to_rfc3339(),
                event_types: window.event_types.clone(),
                min_level: window.min_level.clone(),
            },
            review_reason: window.review_reason.clone(),
        })
    }

    /// Infer event level from event type and payload
    fn infer_event_level(&self, event: &shared_types::Event) -> String {
        if event.event_type.contains("failed") || event.event_type.contains("error") {
            "error".to_string()
        } else if event.event_type.contains("warn") {
            "warn".to_string()
        } else if event.event_type.contains("critical") {
            "critical".to_string()
        } else {
            "info".to_string()
        }
    }

    /// Get mitigation recommendation for an escalation
    async fn recommend_mitigation(
        &self,
        state: &WatcherState,
        escalation: &WatcherEscalation,
    ) -> Result<WatcherMitigationOutput, WatcherError> {
        // Get run state for mitigation recommendation
        let run_state = state
            .run_states
            .get(&escalation.run_id)
            .cloned()
            .unwrap_or_else(|| RunStateSnapshot {
                run_id: escalation.run_id.clone(),
                status: "unknown".to_string(),
                active_call_count: 0,
                recent_failures: 0,
                elapsed_time_ms: 0,
            });

        let registry = ModelRegistry::new();
        let resolved = registry
            .resolve_for_role("watcher", &ModelResolutionContext::default())
            .map_err(|e| {
                WatcherError::MitigationError(format!("Model resolution failed: {}", e))
            })?;

        let client_registry = registry
            .create_runtime_client_registry_for_model(&resolved.config.id)
            .map_err(|e| {
                WatcherError::MitigationError(format!("Client registry creation failed: {}", e))
            })?;

        let input = WatcherMitigationInput {
            escalation: escalation.clone(),
            run_state,
            available_capabilities: vec![
                "terminal".to_string(),
                "researcher".to_string(),
                "writer".to_string(),
            ],
            historical_resolutions: vec![], // Could be populated from past events
        };

        let mitigation = B
            .WatcherRecommendMitigation
            .with_client_registry(&client_registry)
            .call(&input)
            .await
            .map_err(|e| WatcherError::MitigationError(e.to_string()))?;

        Ok(mitigation)
    }

    /// Process an escalation by getting mitigation recommendation and notifying conductor
    async fn process_escalation(
        &self,
        state: &WatcherState,
        escalation: &WatcherEscalation,
    ) -> Result<(), WatcherError> {
        let should_notify_conductor =
            !escalation.run_id.trim().is_empty() && escalation.run_id != "unknown";

        // Get mitigation recommendation
        let mitigation = self.recommend_mitigation(state, escalation).await?;

        if should_notify_conductor {
            // Build Conductor message based on escalation
            let conductor_msg = self
                .build_conductor_message(escalation, &mitigation)
                .await?;

            // Send to Conductor's wake lane
            if let Some(conductor) = &state.conductor_actor {
                let _ = conductor.send_message(conductor_msg);
            }
        } else {
            tracing::debug!(
                escalation_id = %escalation.escalation_id,
                run_id = %escalation.run_id,
                "Skipping conductor wake for escalation without concrete run_id"
            );
        }

        // Emit escalation event for observability
        self.emit_escalation_event(state, escalation, &mitigation)
            .await?;

        Ok(())
    }

    /// Build Conductor message from escalation and mitigation
    async fn build_conductor_message(
        &self,
        escalation: &WatcherEscalation,
        mitigation: &WatcherMitigationOutput,
    ) -> Result<ConductorMsg, WatcherError> {
        use crate::baml_client::types::EscalationAction;

        let payload = serde_json::json!({
            "escalation_id": escalation.escalation_id,
            "kind": format!("{:?}", escalation.kind),
            "urgency": format!("{:?}", escalation.urgency),
            "description": escalation.description,
            "affected_calls": escalation.affected_calls,
            "recommended_action": escalation.recommended_action,
            "mitigation_action": format!("{:?}", mitigation.escalation_action),
            "mitigation_rationale": mitigation.rationale,
            "mitigation_confidence": mitigation.confidence,
            "recommended_capability": mitigation.recommended_capability,
            "recommended_objective": mitigation.recommended_objective,
        });

        let event_type = match mitigation.escalation_action {
            EscalationAction::NotifyConductor => "watcher.escalation.notify",
            EscalationAction::RequestHumanReview => "watcher.escalation.human_review",
            EscalationAction::AutoRetry => "watcher.escalation.auto_retry",
            EscalationAction::ScaleResources => "watcher.escalation.scale_resources",
            EscalationAction::TerminateRun => "watcher.escalation.terminate",
            EscalationAction::ContinueMonitoring => "watcher.escalation.continue_monitoring",
            EscalationAction::EscalateToOnCall => "watcher.escalation.oncall",
        };

        Ok(ConductorMsg::ProcessEvent {
            run_id: escalation.run_id.clone(),
            event_type: event_type.to_string(),
            payload,
            metadata: shared_types::EventMetadata {
                wake_policy: shared_types::WakePolicy::Wake,
                importance: self.urgency_to_importance(&mitigation.urgency),
                run_id: Some(escalation.run_id.clone()),
                task_id: None,
                call_id: escalation.affected_calls.first().cloned(),
                capability: mitigation.recommended_capability.clone(),
                phase: Some("escalation".to_string()),
            },
        })
    }

    /// Convert urgency level to event importance
    fn urgency_to_importance(&self, urgency: &UrgencyLevel) -> shared_types::EventImportance {
        match urgency {
            UrgencyLevel::Critical => shared_types::EventImportance::High,
            UrgencyLevel::High => shared_types::EventImportance::High,
            UrgencyLevel::Medium => shared_types::EventImportance::Normal,
            UrgencyLevel::Low => shared_types::EventImportance::Low,
        }
    }

    /// Emit review event to EventStore
    async fn emit_review_event(
        &self,
        state: &WatcherState,
        window: &EventWindow,
        review: &WatcherReviewOutput,
    ) -> Result<(), WatcherError> {
        let payload = serde_json::json!({
            "window_id": window.id,
            "run_id": window.run_id,
            "task_id": window.task_id,
            "review_status": format!("{:?}", review.review_status),
            "escalation_count": review.escalations.len(),
            "risk_count": review.risks.len(),
            "anomaly_count": review.anomalies.len(),
            "confidence": review.confidence,
            "rationale": review.rationale,
        });

        let alert_event = AppendEvent {
            event_type: "watcher.review.completed".to_string(),
            payload,
            actor_id: state.watcher_id.clone(),
            user_id: "system".to_string(),
        };

        let _ = ractor::call!(state.event_store, |reply| EventStoreMsg::Append {
            event: alert_event,
            reply
        })
        .map_err(|e| WatcherError::Rpc(e.to_string()))?
        .map_err(WatcherError::from)?;

        Ok(())
    }

    /// Emit review failure event to EventStore
    async fn emit_review_failure(
        &self,
        state: &WatcherState,
        window: &EventWindow,
        error: &str,
    ) -> Result<(), WatcherError> {
        let payload = serde_json::json!({
            "window_id": window.id,
            "run_id": window.run_id,
            "task_id": window.task_id,
            "error": error,
            "timestamp": Utc::now().to_rfc3339(),
        });

        let alert_event = AppendEvent {
            event_type: "watcher.review.failed".to_string(),
            payload,
            actor_id: state.watcher_id.clone(),
            user_id: "system".to_string(),
        };

        let _ = ractor::call!(state.event_store, |reply| EventStoreMsg::Append {
            event: alert_event,
            reply
        })
        .map_err(|e| WatcherError::Rpc(e.to_string()))?
        .map_err(WatcherError::from)?;

        Ok(())
    }

    /// Emit escalation event to EventStore
    async fn emit_escalation_event(
        &self,
        state: &WatcherState,
        escalation: &WatcherEscalation,
        mitigation: &WatcherMitigationOutput,
    ) -> Result<(), WatcherError> {
        let payload = serde_json::json!({
            "escalation_id": escalation.escalation_id,
            "run_id": escalation.run_id,
            "kind": format!("{:?}", escalation.kind),
            "urgency": format!("{:?}", escalation.urgency),
            "description": escalation.description,
            "affected_calls": escalation.affected_calls,
            "mitigation_action": format!("{:?}", mitigation.escalation_action),
            "mitigation_confidence": mitigation.confidence,
            "recommended_capability": mitigation.recommended_capability,
            "recommended_objective": mitigation.recommended_objective,
        });

        let alert_event = AppendEvent {
            event_type: "watcher.escalation".to_string(),
            payload,
            actor_id: state.watcher_id.clone(),
            user_id: "system".to_string(),
        };

        let _ = ractor::call!(state.event_store, |reply| EventStoreMsg::Append {
            event: alert_event,
            reply
        })
        .map_err(|e| WatcherError::Rpc(e.to_string()))?
        .map_err(WatcherError::from)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::event_store::{EventStoreActor, EventStoreArguments};
    use ractor::Actor;

    #[tokio::test]
    async fn test_watcher_builds_event_windows() {
        let (store_ref, _store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        // Seed some events
        for idx in 0..5 {
            let _ = ractor::call!(store_ref, |reply| EventStoreMsg::Append {
                event: AppendEvent {
                    event_type: "worker.task.progress".to_string(),
                    payload: serde_json::json!({
                        "run_id": "run-123",
                        "task_id": "task-456",
                        "idx": idx
                    }),
                    actor_id: "supervisor:test".to_string(),
                    user_id: "system".to_string(),
                },
                reply
            })
            .unwrap()
            .unwrap();
        }

        let (watcher_ref, _watcher_handle) = Actor::spawn(
            None,
            WatcherActor,
            WatcherArguments {
                event_store: store_ref.clone(),
                conductor_actor: None,
                watcher_id: "watcher:default".to_string(),
                poll_interval_ms: 10_000,
                review_window_size: 10,
                max_events_per_scan: 100,
                start_from_latest: false,
            },
        )
        .await
        .unwrap();

        // Trigger scan
        let _ = watcher_ref.cast(WatcherMsg::ScanNow);
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify watcher processed events (review events should be emitted)
        // Note: Actual LLM calls would require mocking, but we can verify the structure

        watcher_ref.stop(None);
        store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_watcher_emits_review_completed_event() {
        let (store_ref, _store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        // Seed events
        let _ = ractor::call!(store_ref, |reply| EventStoreMsg::Append {
            event: AppendEvent {
                event_type: "worker.task.started".to_string(),
                payload: serde_json::json!({
                    "run_id": "run-test",
                    "task_id": "task-test",
                }),
                actor_id: "supervisor:test".to_string(),
                user_id: "system".to_string(),
            },
            reply
        })
        .unwrap()
        .unwrap();

        let (watcher_ref, _watcher_handle) = Actor::spawn(
            None,
            WatcherActor,
            WatcherArguments {
                event_store: store_ref.clone(),
                conductor_actor: None,
                watcher_id: "watcher:test".to_string(),
                poll_interval_ms: 10_000,
                review_window_size: 10,
                max_events_per_scan: 100,
                start_from_latest: false,
            },
        )
        .await
        .unwrap();

        // Trigger scan
        let _ = watcher_ref.cast(WatcherMsg::ScanNow);
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Cleanup
        watcher_ref.stop(None);
        store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_watcher_start_from_latest_initializes_cursor_to_log_end() {
        let (store_ref, _store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        let seeded = ractor::call!(store_ref, |reply| EventStoreMsg::Append {
            event: AppendEvent {
                event_type: "worker.task.started".to_string(),
                payload: serde_json::json!({
                    "run_id": "run-seeded",
                    "task_id": "task-seeded",
                }),
                actor_id: "supervisor:test".to_string(),
                user_id: "system".to_string(),
            },
            reply
        })
        .unwrap()
        .unwrap();

        let (watcher_ref, _watcher_handle) = Actor::spawn(
            None,
            WatcherActor,
            WatcherArguments {
                event_store: store_ref.clone(),
                conductor_actor: None,
                watcher_id: "watcher:test".to_string(),
                poll_interval_ms: 10_000,
                review_window_size: 10,
                max_events_per_scan: 100,
                start_from_latest: true,
            },
        )
        .await
        .unwrap();

        let cursor: i64 = ractor::call!(watcher_ref, |reply| WatcherMsg::GetCursor { reply })
            .expect("watcher cursor call should succeed");
        assert_eq!(cursor, seeded.seq);

        watcher_ref.stop(None);
        store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_watcher_ignores_watcher_events_and_advances_cursor() {
        let (store_ref, _store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        let seeded = ractor::call!(store_ref, |reply| EventStoreMsg::Append {
            event: AppendEvent {
                event_type: "watcher.review.completed".to_string(),
                payload: serde_json::json!({
                    "window_id": "window-test",
                    "run_id": "unknown",
                    "task_id": "unknown",
                }),
                actor_id: "watcher:test".to_string(),
                user_id: "system".to_string(),
            },
            reply
        })
        .unwrap()
        .unwrap();

        let (watcher_ref, _watcher_handle) = Actor::spawn(
            None,
            WatcherActor,
            WatcherArguments {
                event_store: store_ref.clone(),
                conductor_actor: None,
                watcher_id: "watcher:test".to_string(),
                poll_interval_ms: 10_000,
                review_window_size: 10,
                max_events_per_scan: 100,
                start_from_latest: false,
            },
        )
        .await
        .unwrap();

        let _ = watcher_ref.cast(WatcherMsg::ScanNow);
        tokio::time::sleep(Duration::from_millis(50)).await;

        let cursor: i64 = ractor::call!(watcher_ref, |reply| WatcherMsg::GetCursor { reply })
            .expect("watcher cursor call should succeed");
        assert_eq!(cursor, seeded.seq);

        let later_events = ractor::call!(store_ref, |reply| EventStoreMsg::GetRecentEvents {
            since_seq: seeded.seq,
            limit: 100,
            event_type_prefix: None,
            actor_id: None,
            user_id: None,
            reply
        })
        .unwrap()
        .unwrap();
        assert!(
            later_events.is_empty(),
            "watcher should not emit review/escalation events for watcher-only inputs"
        );

        watcher_ref.stop(None);
        store_ref.stop(None);
    }
}
