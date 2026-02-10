use shared_types::ConductorOutputMode;

use crate::actors::conductor::output::resolve_output_mode;
use crate::actors::conductor::protocol::WorkerOutput;

#[test]
fn test_resolve_output_mode_auto_prefers_toast_for_brief_output() {
    let output = WorkerOutput {
        report_content: "Short answer line.\n".to_string(),
        citations: vec![],
    };
    assert_eq!(
        resolve_output_mode(ConductorOutputMode::Auto, &output),
        ConductorOutputMode::ToastWithReportLink
    );
}

#[test]
fn test_resolve_output_mode_auto_prefers_report_for_long_output() {
    let output = WorkerOutput {
        report_content: "x".repeat(1600),
        citations: vec![],
    };
    assert_eq!(
        resolve_output_mode(ConductorOutputMode::Auto, &output),
        ConductorOutputMode::MarkdownReportToWriter
    );
}
