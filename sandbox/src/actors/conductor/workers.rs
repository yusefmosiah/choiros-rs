//! Worker call adapters for conductor capability dispatch.

use ractor::ActorRef;
use tokio::sync::mpsc;

use crate::actors::conductor::protocol::ConductorError;
use crate::actors::researcher::{ResearcherMsg, ResearcherProgress, ResearcherResult};
use crate::actors::terminal::{
    ensure_terminal_started, TerminalAgentProgress, TerminalAgentResult, TerminalBashToolRequest,
    TerminalError, TerminalMsg,
};
use crate::actors::writer::WriterMsg;

/// Call the ResearcherActor for an agentic task.
pub async fn call_researcher(
    researcher: &ActorRef<ResearcherMsg>,
    objective: String,
    timeout_ms: Option<u64>,
    max_results: Option<u32>,
    max_rounds: Option<u8>,
    progress_tx: Option<mpsc::UnboundedSender<ResearcherProgress>>,
    writer_actor: Option<ActorRef<WriterMsg>>,
    run_id: Option<String>,
    call_id: Option<String>,
) -> Result<ResearcherResult, ConductorError> {
    use ractor::call;

    call!(researcher, |reply| ResearcherMsg::RunAgenticTask {
        objective,
        timeout_ms,
        max_results,
        max_rounds,
        model_override: None,
        progress_tx,
        writer_actor,
        run_id,
        call_id,
        reply,
    })
    .map_err(|e| ConductorError::WorkerFailed(format!("Failed to call researcher actor: {e}")))?
    .map_err(|e| ConductorError::WorkerFailed(e.to_string()))
}

/// Call the TerminalActor for either a command or an agentic objective.
pub async fn call_terminal(
    terminal: &ActorRef<TerminalMsg>,
    objective: String,
    terminal_command: Option<String>,
    timeout_ms: Option<u64>,
    max_steps: Option<u8>,
    progress_tx: Option<mpsc::UnboundedSender<TerminalAgentProgress>>,
    run_id: Option<String>,
    call_id: Option<String>,
) -> Result<TerminalAgentResult, ConductorError> {
    use ractor::call;

    ensure_terminal_started(terminal)
        .await
        .map_err(ConductorError::WorkerFailed)?;

    if let Some(cmd) = terminal_command {
        call!(terminal, |reply| TerminalMsg::RunBashTool {
            request: TerminalBashToolRequest {
                cmd,
                timeout_ms,
                model_override: None,
                reasoning: Some("conductor capability dispatch terminal command".to_string()),
                run_id,
                call_id,
            },
            progress_tx,
            reply,
        })
        .map_err(|e| {
            ConductorError::WorkerFailed(format!("Failed to call terminal bash tool: {e}"))
        })?
        .map_err(|e| match e {
            TerminalError::Blocked(reason) => ConductorError::WorkerBlocked(reason),
            other => ConductorError::WorkerFailed(other.to_string()),
        })
    } else {
        call!(terminal, |reply| TerminalMsg::RunAgenticTask {
            objective,
            timeout_ms,
            max_steps,
            model_override: None,
            progress_tx,
            writer_actor: None,
            run_id,
            call_id,
            reply,
        })
        .map_err(|e| {
            ConductorError::WorkerFailed(format!("Failed to call terminal agent task: {e}"))
        })?
        .map_err(|e| match e {
            TerminalError::Blocked(reason) => ConductorError::WorkerBlocked(reason),
            other => ConductorError::WorkerFailed(other.to_string()),
        })
    }
}
