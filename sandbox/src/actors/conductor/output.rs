//! Output shaping helpers for conductor run completion.

use shared_types::{ConductorOutputMode, ConductorToastPayload, ConductorToastTone};

use crate::actors::conductor::protocol::WorkerOutput;

pub fn build_worker_output_from_run(run: &shared_types::ConductorRunState) -> WorkerOutput {
    // Read the living document (draft.md) as the source of truth
    let report_content = match std::fs::read_to_string(&run.document_path) {
        Ok(content) => content,
        Err(e) => {
            tracing::error!(run_id = %run.run_id, error = %e, "Failed to read draft document");
            format!("# Error\n\nFailed to read report: {}", e)
        }
    };

    // Extract citations from artifacts if available
    let mut citations = Vec::new();
    for artifact in &run.artifacts {
        if let Some(raw_citations) = artifact
            .metadata
            .as_ref()
            .and_then(|m| m.get("citations"))
            .and_then(|v| v.as_array())
        {
            for raw in raw_citations {
                if let Ok(citation) = serde_json::from_value::<
                    crate::actors::researcher::ResearchCitation,
                >(raw.clone())
                {
                    citations.push(citation);
                }
            }
        }
    }

    WorkerOutput {
        report_content,
        citations,
    }
}

pub fn build_writer_window_props(report_path: &str) -> serde_json::Value {
    serde_json::json!({
        "x": 100,
        "y": 100,
        "width": 900,
        "height": 680,
        "path": report_path,
        "preview_mode": true,
    })
}

pub fn resolve_output_mode(
    requested: ConductorOutputMode,
    output: &WorkerOutput,
) -> ConductorOutputMode {
    match requested {
        ConductorOutputMode::MarkdownReportToWriter => ConductorOutputMode::MarkdownReportToWriter,
        ConductorOutputMode::ToastWithReportLink => ConductorOutputMode::ToastWithReportLink,
        ConductorOutputMode::Auto => {
            if output.report_content.chars().count() <= 900 && output.citations.len() <= 2 {
                ConductorOutputMode::ToastWithReportLink
            } else {
                ConductorOutputMode::MarkdownReportToWriter
            }
        }
    }
}

pub fn build_completion_toast(
    output_mode: ConductorOutputMode,
    output: &WorkerOutput,
    report_path: &str,
) -> Option<ConductorToastPayload> {
    if output_mode != ConductorOutputMode::ToastWithReportLink {
        return None;
    }

    let summary_line = output
        .report_content
        .lines()
        .find(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with("```")
        })
        .unwrap_or("Conductor completed.");
    let message = summary_line.chars().take(240).collect::<String>();

    Some(ConductorToastPayload {
        title: "Conductor Answer".to_string(),
        message,
        tone: ConductorToastTone::Success,
        report_path: Some(report_path.to_string()),
    })
}
