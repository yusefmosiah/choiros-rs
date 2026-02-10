//! Output shaping helpers for conductor run completion.

use shared_types::{ConductorOutputMode, ConductorToastPayload, ConductorToastTone};

use crate::actors::conductor::protocol::WorkerOutput;

pub fn build_worker_output_from_run(run: &shared_types::ConductorRunState) -> WorkerOutput {
    let mut citations = Vec::new();
    let mut sections = Vec::new();

    for artifact in &run.artifacts {
        let summary = artifact
            .metadata
            .as_ref()
            .and_then(|m| m.get("summary"))
            .and_then(|v| v.as_str())
            .unwrap_or("No summary");
        sections.push(format!(
            "- `{}` `{}`: {}",
            artifact.artifact_id,
            format!("{:?}", artifact.kind),
            summary
        ));
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

    let mut report_content = format!(
        "# Conductor Report\n\n## Objective\n\n{}\n\n## Run\n\n- Run ID: `{}`\n- Status: `{}`\n\n## Agenda\n\n",
        run.objective,
        run.run_id,
        format!("{:?}", run.status)
    );
    for item in &run.agenda {
        report_content.push_str(&format!(
            "- `{}` `{}` `{}`\n",
            item.item_id,
            item.capability,
            format!("{:?}", item.status)
        ));
    }
    report_content.push_str("\n## Artifacts\n\n");
    if sections.is_empty() {
        report_content.push_str("- No artifacts produced.\n");
    } else {
        for section in sections {
            report_content.push_str(&section);
            report_content.push('\n');
        }
    }

    if !citations.is_empty() {
        report_content.push_str("\n## Citations\n\n");
        for citation in &citations {
            report_content.push_str(&format!(
                "- [{}]({}) - {}\n",
                citation.title, citation.url, citation.provider
            ));
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
