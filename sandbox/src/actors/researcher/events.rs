use chrono::Utc;
use tokio::sync::mpsc;

use crate::actors::event_store::{AppendEvent, EventStoreMsg};

use super::{ResearcherProgress, ResearcherState};

pub(crate) fn emit_progress(
    state: &ResearcherState,
    progress_tx: &Option<mpsc::UnboundedSender<ResearcherProgress>>,
    loop_id: &str,
    phase: impl Into<String>,
    message: impl Into<String>,
    provider: Option<String>,
    model_used: Option<String>,
    result_count: Option<usize>,
) {
    let phase = phase.into();
    let message = message.into();
    let timestamp = Utc::now().to_rfc3339();
    if let Some(progress_tx) = progress_tx {
        let _ = progress_tx.send(ResearcherProgress {
            phase: phase.clone(),
            message: message.clone(),
            provider: provider.clone(),
            model_used: model_used.clone(),
            result_count,
            timestamp: timestamp.clone(),
        });
    }

    let payload = serde_json::json!({
        "task_id": loop_id,
        "worker_id": state.researcher_id,
        "phase": phase,
        "message": message,
        "provider": provider,
        "model_used": model_used,
        "result_count": result_count,
        "timestamp": timestamp,
    });

    let event = AppendEvent {
        event_type: "worker.task.progress".to_string(),
        payload,
        actor_id: state.researcher_id.clone(),
        user_id: state.user_id.clone(),
    };
    let _ = state
        .event_store
        .send_message(EventStoreMsg::AppendAsync { event });
}
