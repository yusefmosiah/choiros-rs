//! WatcherActor - deterministic event-log monitoring.
//!
//! This actor scans recent EventStore entries and emits watcher alerts for
//! known patterns. It is intentionally deterministic-first.

use async_trait::async_trait;
use chrono::Utc;
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use crate::actors::event_store::{AppendEvent, EventStoreError, EventStoreMsg};

#[derive(Debug, Clone)]
pub struct WatcherArguments {
    pub event_store: ActorRef<EventStoreMsg>,
    pub watcher_id: String,
    pub poll_interval_ms: u64,
    pub failure_spike_threshold: usize,
    pub timeout_spike_threshold: usize,
    pub retry_storm_threshold: usize,
    pub stalled_task_timeout_ms: u64,
}

pub struct WatcherState {
    event_store: ActorRef<EventStoreMsg>,
    watcher_id: String,
    last_seq: i64,
    failure_spike_threshold: usize,
    timeout_spike_threshold: usize,
    retry_storm_threshold: usize,
    stalled_task_timeout_ms: u64,
    pending_tasks: HashMap<String, PendingTask>,
    // Small memory for dedup suppression.
    recent_alert_keys: VecDeque<(String, i64)>,
}

#[derive(Debug, Clone)]
struct PendingTask {
    start_seq: i64,
    started_at: chrono::DateTime<Utc>,
}

#[derive(Debug)]
pub enum WatcherMsg {
    /// Internal trigger to scan the event log.
    ScanNow,
    /// Health/debug endpoint.
    GetCursor { reply: RpcReplyPort<i64> },
}

#[derive(Debug, thiserror::Error)]
pub enum WatcherError {
    #[error("EventStore RPC error: {0}")]
    Rpc(String),
    #[error("EventStore error: {0}")]
    EventStore(String),
}

impl From<EventStoreError> for WatcherError {
    fn from(value: EventStoreError) -> Self {
        Self::EventStore(value.to_string())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct WatcherAlertPayload {
    key: String,
    severity: String,
    message: String,
    rule: String,
    failed_count: usize,
    window_start_seq: i64,
    window_end_seq: i64,
    generated_at: String,
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
            watcher_id: args.watcher_id,
            last_seq: 0,
            failure_spike_threshold: args.failure_spike_threshold.max(1),
            timeout_spike_threshold: args.timeout_spike_threshold.max(1),
            retry_storm_threshold: args.retry_storm_threshold.max(1),
            stalled_task_timeout_ms: args.stalled_task_timeout_ms.max(1_000),
            pending_tasks: HashMap::new(),
            recent_alert_keys: VecDeque::new(),
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
                if let Err(err) = self.scan_and_emit(state).await {
                    tracing::warn!(error = %err, "Watcher scan failed");
                }
            }
            WatcherMsg::GetCursor { reply } => {
                let _ = reply.send(state.last_seq);
            }
        }
        Ok(())
    }
}

impl WatcherActor {
    async fn scan_and_emit(&self, state: &mut WatcherState) -> Result<(), WatcherError> {
        let recent = ractor::call!(state.event_store, |reply| EventStoreMsg::GetRecentEvents {
            since_seq: state.last_seq,
            limit: 500,
            event_type_prefix: None,
            actor_id: None,
            user_id: None,
            reply
        })
        .map_err(|e| WatcherError::Rpc(e.to_string()))?
        .map_err(WatcherError::from)?;

        let mut max_seq = state.last_seq;
        let mut failed_events = Vec::new();
        let mut timeout_failures = Vec::new();
        let mut retry_events = Vec::new();

        if !recent.is_empty() {
            for event in &recent {
                max_seq = max_seq.max(event.seq);
                let task_id = Self::extract_task_id(&event.payload);

                if event.event_type == "worker.task.started" {
                    if let Some(task_id) = &task_id {
                        state.pending_tasks.insert(
                            task_id.clone(),
                            PendingTask {
                                start_seq: event.seq,
                                started_at: event.timestamp,
                            },
                        );
                    }
                }

                if matches!(
                    event.event_type.as_str(),
                    "worker.task.completed" | "worker.task.failed"
                ) {
                    if let Some(task_id) = &task_id {
                        state.pending_tasks.remove(task_id);
                    }
                }

                if event.event_type == "worker.task.failed" {
                    failed_events.push(event.seq);
                    if Self::is_timeout_failure(&event.payload) {
                        timeout_failures.push(event.seq);
                    }
                }

                if event.event_type == "worker.task.progress"
                    && Self::is_retry_progress(&event.payload)
                {
                    retry_events.push(event.seq);
                }
            }

            state.last_seq = max_seq;
        }

        // Rule 1: failure spike in this scan window.
        if failed_events.len() >= state.failure_spike_threshold {
            let key = format!("failure_spike:{}:{}", failed_events[0], failed_events.len());
            if self.should_emit_alert(state, &key) {
                let payload = serde_json::to_value(WatcherAlertPayload {
                    key: key.clone(),
                    severity: "high".to_string(),
                    message: format!(
                        "Detected {} worker failures in a single watcher scan window",
                        failed_events.len()
                    ),
                    rule: "worker_failure_spike".to_string(),
                    failed_count: failed_events.len(),
                    window_start_seq: *failed_events.first().unwrap_or(&state.last_seq),
                    window_end_seq: *failed_events.last().unwrap_or(&state.last_seq),
                    generated_at: Utc::now().to_rfc3339(),
                })
                .unwrap_or_else(|_| serde_json::json!({"error":"serialize"}));
                self.emit_alert(state, "watcher.alert.failure_spike", payload)
                    .await?;
            }
        }

        // Rule 2: timeout spike in this scan window.
        if timeout_failures.len() >= state.timeout_spike_threshold {
            let key = format!(
                "timeout_spike:{}:{}",
                timeout_failures[0],
                timeout_failures.len()
            );
            if self.should_emit_alert(state, &key) {
                let payload = serde_json::json!({
                    "key": key,
                    "severity": "high",
                    "message": format!(
                        "Detected {} timeout-like worker failures in a single watcher scan window",
                        timeout_failures.len()
                    ),
                    "rule": "worker_timeout_spike",
                    "failed_count": timeout_failures.len(),
                    "window_start_seq": timeout_failures.first().copied().unwrap_or(state.last_seq),
                    "window_end_seq": timeout_failures.last().copied().unwrap_or(state.last_seq),
                    "generated_at": Utc::now().to_rfc3339(),
                });
                self.emit_alert(state, "watcher.alert.timeout_spike", payload)
                    .await?;
            }
        }

        // Rule 3: started tasks that have not reached completion/failure in time.
        if retry_events.len() >= state.retry_storm_threshold {
            let key = format!("retry_storm:{}:{}", retry_events[0], retry_events.len());
            if self.should_emit_alert(state, &key) {
                let payload = serde_json::json!({
                    "key": key,
                    "severity": "medium",
                    "message": format!(
                        "Detected {} retry-like worker progress updates in a single watcher scan window",
                        retry_events.len()
                    ),
                    "rule": "worker_retry_storm",
                    "retry_count": retry_events.len(),
                    "window_start_seq": retry_events.first().copied().unwrap_or(state.last_seq),
                    "window_end_seq": retry_events.last().copied().unwrap_or(state.last_seq),
                    "generated_at": Utc::now().to_rfc3339(),
                });
                self.emit_alert(state, "watcher.alert.retry_storm", payload)
                    .await?;
            }
        }

        // Rule 4: started tasks that have not reached completion/failure in time.
        let now = Utc::now();
        let stalled_after_ms = i64::try_from(state.stalled_task_timeout_ms).unwrap_or(i64::MAX);
        let stalled: Vec<(String, PendingTask)> = state
            .pending_tasks
            .iter()
            .filter_map(|(task_id, pending)| {
                let elapsed_ms = (now - pending.started_at).num_milliseconds();
                if elapsed_ms >= stalled_after_ms {
                    Some((task_id.clone(), pending.clone()))
                } else {
                    None
                }
            })
            .collect();

        for (task_id, pending) in stalled {
            let key = format!("stalled_task:{task_id}:{}", pending.start_seq);
            if self.should_emit_alert(state, &key) {
                let payload = serde_json::json!({
                    "key": key,
                    "severity": "medium",
                    "message": format!("Task {task_id} has not completed or failed within {}ms", state.stalled_task_timeout_ms),
                    "rule": "worker_stalled_task",
                    "task_id": task_id,
                    "start_seq": pending.start_seq,
                    "started_at": pending.started_at.to_rfc3339(),
                    "stalled_timeout_ms": state.stalled_task_timeout_ms,
                    "generated_at": now.to_rfc3339(),
                });
                self.emit_alert(state, "watcher.alert.stalled_task", payload)
                    .await?;
            }
        }

        Ok(())
    }

    async fn emit_alert(
        &self,
        state: &WatcherState,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Result<(), WatcherError> {
        let alert_event = AppendEvent {
            event_type: event_type.to_string(),
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

    fn extract_task_id(payload: &serde_json::Value) -> Option<String> {
        if let Some(task_id) = payload.get("task_id").and_then(|v| v.as_str()) {
            return Some(task_id.to_string());
        }
        payload
            .get("task")
            .and_then(|v| v.get("task_id"))
            .and_then(|v| v.as_str())
            .map(|v| v.to_string())
    }

    fn is_timeout_failure(payload: &serde_json::Value) -> bool {
        let error = payload
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        error.contains("timeout")
            || error.contains("timed out")
            || error.contains("deadline")
            || error.contains("did not return within")
    }

    fn is_retry_progress(payload: &serde_json::Value) -> bool {
        let phase = payload
            .get("phase")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let status = payload
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let message = payload
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        phase.contains("retry") || status.contains("retry") || message.contains("retry")
    }

    fn should_emit_alert(&self, state: &mut WatcherState, key: &str) -> bool {
        let current_seq = state.last_seq;
        // Dedup for ~10k events worth of sequence progress.
        let dedup_window = 10_000_i64;

        while let Some((_, seq)) = state.recent_alert_keys.front() {
            if current_seq - *seq > dedup_window {
                state.recent_alert_keys.pop_front();
            } else {
                break;
            }
        }

        if state.recent_alert_keys.iter().any(|(k, _)| k == key) {
            return false;
        }

        state
            .recent_alert_keys
            .push_back((key.to_string(), current_seq.max(1)));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::event_store::{get_recent_events, EventStoreActor, EventStoreArguments};
    use ractor::Actor;

    #[tokio::test]
    async fn test_watcher_emits_failure_spike_alert() {
        let (store_ref, _store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        // Seed failure events.
        for idx in 0..3 {
            let _ = ractor::call!(store_ref, |reply| EventStoreMsg::Append {
                event: AppendEvent {
                    event_type: "worker.task.failed".to_string(),
                    payload: serde_json::json!({"idx": idx}),
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
                watcher_id: "watcher:default".to_string(),
                poll_interval_ms: 10_000, // keep background loop effectively inactive for test
                failure_spike_threshold: 3,
                timeout_spike_threshold: 2,
                retry_storm_threshold: 3,
                stalled_task_timeout_ms: 60_000,
            },
        )
        .await
        .unwrap();

        let _ = watcher_ref.cast(WatcherMsg::ScanNow);
        tokio::time::sleep(Duration::from_millis(50)).await;

        let alerts = get_recent_events(
            &store_ref,
            0,
            50,
            Some("watcher.alert".to_string()),
            Some("watcher:default".to_string()),
            None,
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].event_type, "watcher.alert.failure_spike");

        watcher_ref.stop(None);
        store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_watcher_emits_timeout_spike_alert() {
        let (store_ref, _store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        for idx in 0..2 {
            let _ = ractor::call!(store_ref, |reply| EventStoreMsg::Append {
                event: AppendEvent {
                    event_type: "worker.task.failed".to_string(),
                    payload: serde_json::json!({
                        "task_id": format!("t-timeout-{idx}"),
                        "error": "terminal agent did not return within 40000ms"
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
                watcher_id: "watcher:default".to_string(),
                poll_interval_ms: 10_000,
                failure_spike_threshold: 99,
                timeout_spike_threshold: 2,
                retry_storm_threshold: 99,
                stalled_task_timeout_ms: 60_000,
            },
        )
        .await
        .unwrap();

        let _ = watcher_ref.cast(WatcherMsg::ScanNow);
        tokio::time::sleep(Duration::from_millis(50)).await;

        let alerts = get_recent_events(
            &store_ref,
            0,
            50,
            Some("watcher.alert.timeout_spike".to_string()),
            Some("watcher:default".to_string()),
            None,
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].event_type, "watcher.alert.timeout_spike");

        watcher_ref.stop(None);
        store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_watcher_emits_stalled_task_alert() {
        let (store_ref, _store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        let _ = ractor::call!(store_ref, |reply| EventStoreMsg::Append {
            event: AppendEvent {
                event_type: "worker.task.started".to_string(),
                payload: serde_json::json!({
                    "task": { "task_id": "task-stalled-1" },
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
                watcher_id: "watcher:default".to_string(),
                poll_interval_ms: 10_000,
                failure_spike_threshold: 99,
                timeout_spike_threshold: 99,
                retry_storm_threshold: 99,
                stalled_task_timeout_ms: 1_000,
            },
        )
        .await
        .unwrap();

        tokio::time::sleep(Duration::from_millis(1_100)).await;
        let _ = watcher_ref.cast(WatcherMsg::ScanNow);
        tokio::time::sleep(Duration::from_millis(50)).await;

        let alerts = get_recent_events(
            &store_ref,
            0,
            50,
            Some("watcher.alert.stalled_task".to_string()),
            Some("watcher:default".to_string()),
            None,
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].event_type, "watcher.alert.stalled_task");

        watcher_ref.stop(None);
        store_ref.stop(None);
    }

    #[tokio::test]
    async fn test_watcher_emits_retry_storm_alert() {
        let (store_ref, _store_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .unwrap();

        for idx in 0..3 {
            let _ = ractor::call!(store_ref, |reply| EventStoreMsg::Append {
                event: AppendEvent {
                    event_type: "worker.task.progress".to_string(),
                    payload: serde_json::json!({
                        "task_id": format!("t-retry-{idx}"),
                        "phase": "retry_attempt",
                        "message": "retrying step"
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
                watcher_id: "watcher:default".to_string(),
                poll_interval_ms: 10_000,
                failure_spike_threshold: 99,
                timeout_spike_threshold: 99,
                retry_storm_threshold: 3,
                stalled_task_timeout_ms: 60_000,
            },
        )
        .await
        .unwrap();

        let _ = watcher_ref.cast(WatcherMsg::ScanNow);
        tokio::time::sleep(Duration::from_millis(50)).await;

        let alerts = get_recent_events(
            &store_ref,
            0,
            50,
            Some("watcher.alert.retry_storm".to_string()),
            Some("watcher:default".to_string()),
            None,
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].event_type, "watcher.alert.retry_storm");

        watcher_ref.stop(None);
        store_ref.stop(None);
    }
}
