use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, ActorRef};

use crate::actors::conductor::model_gateway::SharedConductorModelGateway;
use crate::actors::conductor::protocol::{CapabilityWorkerOutput, ConductorError, ConductorMsg};
use crate::actors::writer::{SectionState, WriterInboundEnvelope, WriterMsg, WriterSource};

#[derive(Debug, Default)]
pub(crate) struct CapabilityCallActor;

#[derive(Clone)]
pub(crate) struct CapabilityCallArguments {
    pub conductor_ref: ActorRef<ConductorMsg>,
    pub model_gateway: SharedConductorModelGateway,
    pub writer_actor: Option<ActorRef<WriterMsg>>,
    pub run_id: String,
    pub call_id: String,
    pub agenda_item_id: String,
    pub capability: String,
    pub objective: String,
}

#[derive(Debug)]
pub(crate) enum CapabilityCallMsg {
    Run,
}

#[derive(Clone)]
pub(crate) struct CapabilityCallState {
    pub conductor_ref: ActorRef<ConductorMsg>,
    pub model_gateway: SharedConductorModelGateway,
    pub writer_actor: Option<ActorRef<WriterMsg>>,
    pub run_id: String,
    pub call_id: String,
    pub agenda_item_id: String,
    pub capability: String,
    pub objective: String,
}

#[async_trait]
impl Actor for CapabilityCallActor {
    type Msg = CapabilityCallMsg;
    type State = CapabilityCallState;
    type Arguments = CapabilityCallArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let state = CapabilityCallState {
            conductor_ref: args.conductor_ref,
            model_gateway: args.model_gateway,
            writer_actor: args.writer_actor,
            run_id: args.run_id,
            call_id: args.call_id,
            agenda_item_id: args.agenda_item_id,
            capability: args.capability,
            objective: args.objective,
        };
        let _ = myself.send_message(CapabilityCallMsg::Run);
        Ok(state)
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            CapabilityCallMsg::Run => {
                let result = run_capability_call(state.clone()).await;
                if !matches!(result, Ok(CapabilityWorkerOutput::ImmediateResponse(_))) {
                    if let Some(writer_actor) = state.writer_actor.clone() {
                        emit_result_to_writer(&writer_actor, state, &result).await;
                    }
                }
                let _ = state
                    .conductor_ref
                    .send_message(ConductorMsg::CapabilityCallFinished {
                        run_id: state.run_id.clone(),
                        call_id: state.call_id.clone(),
                        agenda_item_id: state.agenda_item_id.clone(),
                        capability: state.capability.clone(),
                        result,
                    });
                myself.stop(None);
            }
        }
        Ok(())
    }
}

async fn run_capability_call(
    state: CapabilityCallState,
) -> Result<CapabilityWorkerOutput, ConductorError> {
    let capability = state.capability.to_ascii_lowercase();
    if capability == "immediate_response" {
        let message = state
            .model_gateway
            .immediate_response(Some(&state.run_id), &state.objective)
            .await?;
        return Ok(CapabilityWorkerOutput::ImmediateResponse(message));
    }

    let writer_actor = state.writer_actor.ok_or_else(|| {
        ConductorError::WorkerFailed(
            "Writer actor unavailable for capability delegation".to_string(),
        )
    })?;

    if capability != "writer" {
        return Err(ConductorError::WorkerFailed(format!(
            "Conductor cannot dispatch worker capability '{capability}' directly; route through writer"
        )));
    }
    let orchestration_result =
        ractor::call!(writer_actor, |reply| WriterMsg::OrchestrateObjective {
            objective: state.objective,
            timeout_ms: Some(60_000),
            max_steps: Some(100),
            run_id: Some(state.run_id),
            call_id: Some(state.call_id),
            reply,
        })
        .map_err(|e| ConductorError::WorkerFailed(e.to_string()))?
        .map_err(|e| ConductorError::WorkerFailed(e.to_string()))?;
    Ok(CapabilityWorkerOutput::Writer(orchestration_result))
}

fn writer_section_for_capability(capability: &str) -> String {
    match capability {
        "researcher" | "terminal" | "writer" => capability.to_string(),
        _ => "conductor".to_string(),
    }
}

fn writer_source_for_capability(capability: &str) -> WriterSource {
    match capability {
        "researcher" => WriterSource::Researcher,
        "terminal" => WriterSource::Terminal,
        "writer" => WriterSource::Writer,
        _ => WriterSource::Conductor,
    }
}

async fn emit_result_to_writer(
    writer_actor: &ActorRef<WriterMsg>,
    state: &CapabilityCallState,
    result: &Result<CapabilityWorkerOutput, ConductorError>,
) {
    let capability = state.capability.to_ascii_lowercase();
    let section_id = writer_section_for_capability(&capability);
    let source = writer_source_for_capability(&capability);

    let (section_state, kind, content) = match result {
        Ok(CapabilityWorkerOutput::Researcher(output)) => {
            let mut writer_content = format!(
                "Researcher capability completed.\nSummary: {}\nObjective status: {:?}\nCompletion reason: {}",
                output.summary, output.objective_status, output.completion_reason
            );
            if let Some(next_capability) = output.recommended_next_capability.as_ref() {
                writer_content
                    .push_str(&format!("\nRecommended next capability: {next_capability}"));
            }
            if let Some(next_objective) = output.recommended_next_objective.as_ref() {
                writer_content.push_str(&format!("\nRecommended next objective: {next_objective}"));
            }
            if !output.citations.is_empty() {
                writer_content.push_str("\nCitations:");
                for citation in output.citations.iter().take(8) {
                    writer_content.push_str(&format!("\n- [{}]({})", citation.title, citation.url));
                }
            }
            (
                SectionState::Complete,
                "capability_completed",
                writer_content,
            )
        }
        Ok(CapabilityWorkerOutput::Terminal(output)) => {
            if output.success {
                let mut writer_content = format!(
                    "Terminal capability completed.\nSummary: {}",
                    output.summary
                );
                if let Some(reasoning) = output.reasoning.as_ref() {
                    writer_content.push_str(&format!("\nReasoning: {reasoning}"));
                }
                if !output.executed_commands.is_empty() {
                    writer_content.push_str("\nExecuted commands:");
                    for command in output.executed_commands.iter().take(10) {
                        writer_content.push_str(&format!("\n- `{command}`"));
                    }
                }
                (
                    SectionState::Complete,
                    "capability_completed",
                    writer_content,
                )
            } else {
                let writer_content =
                    format!("Terminal capability failed.\nSummary: {}", output.summary);
                (SectionState::Failed, "capability_failed", writer_content)
            }
        }
        Ok(CapabilityWorkerOutput::Writer(output)) => {
            let delegated = if output.delegated_capabilities.is_empty() {
                "none".to_string()
            } else {
                output.delegated_capabilities.join(", ")
            };
            let has_pending_delegations = output.pending_delegations > 0;
            let writer_content = if has_pending_delegations {
                format!(
                    "Writer orchestration dispatched worker delegation.\nSummary: {}\nDelegated capabilities: {}\nPending delegations: {}",
                    output.summary, delegated, output.pending_delegations
                )
            } else {
                format!(
                    "Writer orchestration completed.\nSummary: {}\nDelegated capabilities: {}",
                    output.summary, delegated
                )
            };
            if has_pending_delegations {
                (SectionState::Running, "capability_progress", writer_content)
            } else if output.success {
                (
                    SectionState::Complete,
                    "capability_completed",
                    writer_content,
                )
            } else {
                (SectionState::Failed, "capability_failed", writer_content)
            }
        }
        Ok(CapabilityWorkerOutput::ImmediateResponse(_)) => {
            return;
        }
        Ok(CapabilityWorkerOutput::Subharness(_)) => {
            let writer_content = format!(
                "Capability failed.\nCapability: {}\nError: unsupported capability output",
                capability
            );
            (SectionState::Failed, "capability_failed", writer_content)
        }
        Err(err) => {
            let writer_content = format!(
                "Capability failed.\nCapability: {}\nError: {}",
                capability, err
            );
            (SectionState::Failed, "capability_failed", writer_content)
        }
    };

    let _ = ractor::call!(writer_actor, |reply| WriterMsg::SetSectionState {
        run_id: state.run_id.clone(),
        section_id: section_id.clone(),
        state: section_state,
        reply,
    });

    if content.trim().is_empty() {
        return;
    }

    let message_id = format!(
        "worker:{}:{}:{}:{}",
        state.run_id, state.call_id, kind, capability
    );
    let envelope = WriterInboundEnvelope {
        message_id,
        correlation_id: state.call_id.clone(),
        kind: kind.to_string(),
        run_id: state.run_id.clone(),
        section_id,
        source,
        content,
        base_version_id: None,
        prompt_diff: None,
        overlay_id: None,
        session_id: None,
        thread_id: None,
        call_id: Some(state.call_id.clone()),
        origin_actor: Some("capability_call".to_string()),
    };
    let _ = ractor::call!(writer_actor, |reply| WriterMsg::EnqueueInbound {
        envelope,
        reply
    });
}
