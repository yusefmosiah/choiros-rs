//! Worker call adapters for conductor capability dispatch.

use ractor::ActorRef;

use crate::actors::conductor::protocol::ConductorError;
use crate::actors::researcher::{ResearcherMsg, ResearcherResult};
use crate::actors::terminal::{
    TerminalAgentResult, TerminalBashToolRequest, TerminalError, TerminalMsg,
};

/// Call the ResearcherActor for an agentic task.
pub async fn call_researcher(
    researcher: &ActorRef<ResearcherMsg>,
    objective: String,
    timeout_ms: Option<u64>,
    max_results: Option<u32>,
    max_rounds: Option<u8>,
) -> Result<ResearcherResult, ConductorError> {
    use ractor::call;

    call!(researcher, |reply| ResearcherMsg::RunAgenticTask {
        objective,
        timeout_ms,
        max_results,
        max_rounds,
        model_override: None,
        progress_tx: None,
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
) -> Result<TerminalAgentResult, ConductorError> {
    use ractor::call;

    match call!(terminal, |reply| TerminalMsg::Start { reply }) {
        Ok(Ok(())) | Ok(Err(TerminalError::AlreadyRunning)) => {}
        Ok(Err(e)) => {
            return Err(ConductorError::WorkerFailed(format!(
                "Failed to start terminal: {e}"
            )));
        }
        Err(e) => {
            return Err(ConductorError::WorkerFailed(format!(
                "Failed to call terminal start: {e}"
            )));
        }
    }

    if let Some(cmd) = terminal_command {
        call!(terminal, |reply| TerminalMsg::RunBashTool {
            request: TerminalBashToolRequest {
                cmd,
                timeout_ms,
                model_override: None,
                reasoning: Some("conductor capability dispatch terminal command".to_string()),
            },
            progress_tx: None,
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
            progress_tx: None,
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
