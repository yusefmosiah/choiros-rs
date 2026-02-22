use dioxus::prelude::*;
use shared_types::{
    ConductorError, ConductorExecuteResponse, ConductorOutputMode, ConductorRunStatus,
    ConductorToastPayload, EventImportance, WindowState, WriterWindowProps,
};

use crate::api::{
    conductor_get_run_state, conductor_get_run_status, execute_conductor, open_window,
};
use crate::desktop::apps::get_app_icon;

// ============================================================================
// Phase F: Live Telemetry Stream (Star Wars Style Rising Lines)
// ============================================================================

/// A single telemetry line for display
#[derive(Clone, Debug)]
pub struct TelemetryLine {
    pub id: String,
    pub message: String,
    pub capability: String,
    pub phase: String,
    pub importance: EventImportance,
    pub created_at_ms: f64, // JS timestamp in milliseconds
    pub ttl_ms: u32,
}

impl TelemetryLine {
    fn now_ms() -> f64 {
        // Use JS Date.now() equivalent through wasm-bindgen if available,
        // otherwise fall back to performance.now approximation
        #[cfg(target_arch = "wasm32")]
        {
            js_sys::Date::now()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as f64
        }
    }

    /// Calculate current opacity based on age (fade out over TTL)
    pub fn opacity(&self) -> f64 {
        let age_ms = Self::now_ms() - self.created_at_ms;
        let ttl = self.ttl_ms as f64;
        if age_ms >= ttl {
            0.0
        } else {
            1.0 - (age_ms / ttl)
        }
    }

    /// Calculate vertical offset (rise up over time)
    pub fn vertical_offset(&self) -> f64 {
        let age_ms = Self::now_ms() - self.created_at_ms;
        let progress = (age_ms / 5000.0).min(1.0); // Full rise over 5s
        -progress * 60.0 // Rise up by 60px
    }

    /// Calculate scale (shrink as it rises)
    pub fn scale(&self) -> f64 {
        let age_ms = Self::now_ms() - self.created_at_ms;
        let progress = (age_ms / 5000.0).min(1.0);
        1.0 - (progress * 0.2) // Shrink to 80%
    }

    pub fn is_expired(&self) -> bool {
        (Self::now_ms() - self.created_at_ms) as u32 >= self.ttl_ms
    }
}

/// State for the live telemetry stream
#[derive(Clone, Debug, Default)]
pub struct TelemetryStreamState {
    pub lines: Vec<TelemetryLine>,
    pub max_lines: usize,
    pub enabled: bool,
    next_id: u64,
}

impl TelemetryStreamState {
    pub fn new(max_lines: usize) -> Self {
        Self {
            lines: Vec::new(),
            max_lines,
            enabled: true,
            next_id: 0,
        }
    }

    pub fn add_line(
        &mut self,
        message: String,
        capability: String,
        phase: String,
        importance: EventImportance,
    ) {
        let now_ms = TelemetryLine::now_ms();
        self.next_id = self.next_id.saturating_add(1);
        let line = TelemetryLine {
            id: format!("{}-{}", capability, self.next_id),
            message,
            capability,
            phase,
            importance,
            created_at_ms: now_ms,
            ttl_ms: match importance {
                EventImportance::High => 8000,
                EventImportance::Normal => 5000,
                EventImportance::Low => 3000,
            },
        };
        self.lines.push(line);
        // Keep only the most recent lines
        if self.lines.len() > self.max_lines {
            self.lines.remove(0);
        }
    }

    pub fn cleanup_expired(&mut self) {
        self.lines.retain(|line| !line.is_expired());
    }
}

/// Props for the telemetry stream component
#[derive(Props, Clone, PartialEq)]
pub struct LiveTelemetryStreamProps {
    pub state: Signal<TelemetryStreamState>,
    pub is_active: bool,
}

/// Live telemetry stream component - displays rising animated lines
#[component]
pub fn LiveTelemetryStream(props: LiveTelemetryStreamProps) -> Element {
    // Spawn cleanup task
    let mut state = props.state;
    use_effect(move || {
        spawn(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(100).await; // 100ms cleanup interval
                state.write().cleanup_expired();
            }
        });
    });

    let state_read = state.read();

    if !state_read.enabled || state_read.lines.is_empty() {
        return rsx! {};
    }

    rsx! {
        div {
            class: "live-telemetry-stream",
            style: "position: absolute; bottom: 100%; left: 0; right: 0; height: 200px; overflow: visible; pointer-events: none; z-index: 1500;",

            for line in state_read.lines.iter().rev().enumerate() {
                div {
                    key: "{line.1.id}-{line.0}",
                    class: "telemetry-line",
                    style: format!(
                        "position: absolute; bottom: 0; left: 1rem; right: 1rem; padding: 0.25rem 0.5rem; font-size: 0.75rem; font-family: monospace; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; transform: translateY({}px) scale({}); opacity: {}; color: {}; transition: transform 0.1s linear, opacity 0.1s linear;",
                        line.1.vertical_offset(),
                        line.1.scale(),
                        line.1.opacity(),
                        match line.1.importance {
                            EventImportance::High => "#f59e0b", // amber
                            EventImportance::Normal => "#10b981", // emerald
                            EventImportance::Low => "#6b7280",   // gray
                        }
                    ),

                    // Capability indicator
                    span {
                        style: "display: inline-block; padding: 0.1rem 0.25rem; background: rgba(59, 130, 246, 0.2); color: #60a5fa; border-radius: 2px; margin-right: 0.5rem; font-size: 0.65rem; text-transform: uppercase;",
                        "[{line.1.capability}:{line.1.phase}]"
                    }

                    // Message
                    span { "{line.1.message}" }
                }
            }
        }
    }
}

/// Helper function to format telemetry messages for display
pub fn format_telemetry_message(event_type: &str, data: &serde_json::Value) -> String {
    // The actual payload fields are nested under the "data" key
    let payload = data.get("data").unwrap_or(data);

    match event_type {
        "conductor.tool.call" => {
            let tool = payload
                .get("tool")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            format!("Calling {}...", tool)
        }
        "conductor.tool.result" => {
            let tool = payload
                .get("tool")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let success = payload
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if success {
                format!("{} completed", tool)
            } else {
                format!("{} failed", tool)
            }
        }
        "conductor.progress" => payload
            .get("message")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| "Processing...".to_string()),
        "conductor.finding" => {
            let claim = payload
                .get("claim")
                .and_then(|v| v.as_str())
                .unwrap_or("New finding");
            format!("Found: {}", claim.chars().take(40).collect::<String>())
        }
        "conductor.learning" => {
            let insight = payload
                .get("insight")
                .and_then(|v| v.as_str())
                .unwrap_or("New insight");
            format!("Learned: {}", insight.chars().take(40).collect::<String>())
        }
        "conductor.capability.completed" => {
            format!("Capability completed")
        }
        "conductor.capability.failed" => {
            format!("Capability failed")
        }
        _ => {
            // Generic message
            event_type.replace("conductor.", "").replace(".", " ")
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConductorSubmissionState {
    Idle,
    Submitting,
    OpeningWriter {
        run_id: String,
    },
    Success {
        run_id: String,
    },
    ToastReady {
        run_id: String,
        toast: ConductorToastPayload,
    },
    Failed {
        code: String,
        message: String,
    },
}

impl ConductorSubmissionState {
    pub fn display_text(&self) -> Option<&'static str> {
        match self {
            ConductorSubmissionState::Idle => None,
            ConductorSubmissionState::Submitting => Some("Submitting..."),
            ConductorSubmissionState::OpeningWriter { .. } => Some("Opening Writer..."),
            ConductorSubmissionState::Success { .. } => None,
            ConductorSubmissionState::ToastReady { .. } => None,
            ConductorSubmissionState::Failed { .. } => None,
        }
    }

    pub fn is_active(&self) -> bool {
        !matches!(
            self,
            ConductorSubmissionState::Idle
                | ConductorSubmissionState::Success { .. }
                | ConductorSubmissionState::ToastReady { .. }
                | ConductorSubmissionState::Failed { .. }
        )
    }

    pub fn error_message(&self) -> Option<String> {
        match self {
            ConductorSubmissionState::Failed { code, message } => {
                Some(format!("{}: {}", code, message))
            }
            _ => None,
        }
    }

    pub fn success_message(&self) -> Option<&'static str> {
        match self {
            ConductorSubmissionState::Success { .. } => Some("Writer opened"),
            _ => None,
        }
    }

    pub fn toast_payload(&self) -> Option<&ConductorToastPayload> {
        match self {
            ConductorSubmissionState::ToastReady { toast, .. } => Some(toast),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskLifecycleDecision {
    Running,
    Completed,
    Failed,
}

fn classify_run_status(status: ConductorRunStatus) -> TaskLifecycleDecision {
    match status {
        ConductorRunStatus::Initializing
        | ConductorRunStatus::Running
        | ConductorRunStatus::WaitingForCalls
        | ConductorRunStatus::Completing => TaskLifecycleDecision::Running,
        ConductorRunStatus::Completed => TaskLifecycleDecision::Completed,
        ConductorRunStatus::Failed | ConductorRunStatus::Blocked => TaskLifecycleDecision::Failed,
    }
}

fn failure_from_error(error: Option<ConductorError>) -> (String, String) {
    error.map(|e| (e.code, e.message)).unwrap_or_else(|| {
        (
            "RUN_FAILED".to_string(),
            "Run failed without error details".to_string(),
        )
    })
}

fn run_state_requires_writer(run: &shared_types::ConductorRunState) -> bool {
    run.agenda
        .iter()
        .any(|item| !item.capability.eq_ignore_ascii_case("immediate_response"))
}

fn writer_props_for_run_document(document_path: &str, run_id: &str) -> WriterWindowProps {
    WriterWindowProps {
        x: 100,
        y: 100,
        width: 900,
        height: 680,
        path: document_path.to_string(),
        preview_mode: false,
        run_id: Some(run_id.to_string()),
    }
}

fn writer_props_for_report(report_path: &str, run_id: &str) -> WriterWindowProps {
    WriterWindowProps {
        x: 100,
        y: 100,
        width: 900,
        height: 680,
        path: report_path.to_string(),
        preview_mode: true,
        run_id: Some(run_id.to_string()),
    }
}

fn toast_style_for_tone(tone: shared_types::ConductorToastTone) -> &'static str {
    match tone {
        shared_types::ConductorToastTone::Info => {
            "position: absolute; right: 0.375rem; top: 0.375rem; bottom: 0.375rem; display: flex; align-items: center; padding: 0 0.75rem; background: #60a5fa; color: #0b1020; border: 1px solid #3b82f6; border-radius: var(--radius-md, 8px); font-size: 0.75rem; font-weight: 600; max-width: 70%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; cursor: pointer;"
        }
        shared_types::ConductorToastTone::Success => {
            "position: absolute; right: 0.375rem; top: 0.375rem; bottom: 0.375rem; display: flex; align-items: center; padding: 0 0.75rem; background: #34d399; color: #042f2e; border: 1px solid #10b981; border-radius: var(--radius-md, 8px); font-size: 0.75rem; font-weight: 600; max-width: 70%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; cursor: pointer;"
        }
        shared_types::ConductorToastTone::Warning => {
            "position: absolute; right: 0.375rem; top: 0.375rem; bottom: 0.375rem; display: flex; align-items: center; padding: 0 0.75rem; background: #fbbf24; color: #111827; border: 1px solid #f59e0b; border-radius: var(--radius-md, 8px); font-size: 0.75rem; font-weight: 600; max-width: 70%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; cursor: pointer;"
        }
        shared_types::ConductorToastTone::Error => {
            "position: absolute; right: 0.375rem; top: 0.375rem; bottom: 0.375rem; display: flex; align-items: center; padding: 0 0.75rem; background: #f87171; color: #111827; border: 1px solid #ef4444; border-radius: var(--radius-md, 8px); font-size: 0.75rem; font-weight: 600; max-width: 70%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; cursor: pointer;"
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct PromptBarProps {
    pub connected: bool,
    pub is_mobile: bool,
    pub windows: Vec<WindowState>,
    pub active_window: Option<String>,
    pub desktop_id: String,
    pub on_focus_window: Callback<String>,
    pub on_show_desktop: Callback<()>,
    pub current_theme: String,
    pub on_toggle_theme: Callback<()>,
    pub telemetry_state: Signal<TelemetryStreamState>,
}

#[component]
pub fn PromptBar(props: PromptBarProps) -> Element {
    let connected = props.connected;
    let is_mobile = props.is_mobile;
    let windows = props.windows;
    let active_window = props.active_window;
    let desktop_id = props.desktop_id;
    let on_focus_window = props.on_focus_window;
    let on_show_desktop = props.on_show_desktop;
    let current_theme = props.current_theme;
    let on_toggle_theme = props.on_toggle_theme;
    let telemetry_state = props.telemetry_state;
    let mut input_value = use_signal(String::new);
    let mut prompt_expanded = use_signal(|| true);
    let mut conductor_state = use_signal(|| ConductorSubmissionState::Idle);
    let desktop_id_signal = use_signal(|| desktop_id.clone());

    // Conductor submission handler
    let handle_conductor_submit = use_callback(move |objective: String| {
        let desktop_id = desktop_id_signal.read().clone();
        let mut state = conductor_state.clone();
        prompt_expanded.set(true);

        if state().is_active() {
            return;
        }
        state.set(ConductorSubmissionState::Submitting);

        spawn(async move {
            // Execute conductor task
            match execute_conductor(&objective, &desktop_id, ConductorOutputMode::Auto).await {
                Ok(response) => {
                    handle_conductor_response(response, state, desktop_id).await;
                }
                Err(e) => {
                    state.set(ConductorSubmissionState::Failed {
                        code: "EXECUTE_FAILED".to_string(),
                        message: e,
                    });
                }
            }
        });
    });

    async fn handle_conductor_response(
        response: ConductorExecuteResponse,
        mut state: Signal<ConductorSubmissionState>,
        desktop_id: String,
    ) {
        match classify_run_status(response.status) {
            TaskLifecycleDecision::Running => {
                if response.writer_window_props.is_some() {
                    state.set(ConductorSubmissionState::OpeningWriter {
                        run_id: response.run_id.clone(),
                    });

                    if let Err(e) =
                        open_writer_window(&desktop_id, response.writer_window_props).await
                    {
                        state.set(ConductorSubmissionState::Failed {
                            code: "WINDOW_OPEN_FAILED".to_string(),
                            message: e,
                        });
                    } else {
                        state.set(ConductorSubmissionState::Success {
                            run_id: response.run_id,
                        });
                    }
                } else {
                    state.set(ConductorSubmissionState::Submitting);
                    let run_id = response.run_id.clone();
                    let mut opened_writer = false;

                    for _ in 0..80 {
                        match conductor_get_run_status(&run_id).await {
                            Ok(run_status) => match classify_run_status(run_status.status) {
                                TaskLifecycleDecision::Completed => {
                                    if let Some(toast) = run_status.toast {
                                        state.set(ConductorSubmissionState::ToastReady {
                                            run_id: run_id.clone(),
                                            toast,
                                        });
                                        return;
                                    }

                                    let writer_props = run_status
                                        .report_path
                                        .as_deref()
                                        .map(|path| writer_props_for_report(path, &run_id))
                                        .or_else(|| {
                                            (!run_status.document_path.trim().is_empty()).then(
                                                || {
                                                    writer_props_for_run_document(
                                                        &run_status.document_path,
                                                        &run_id,
                                                    )
                                                },
                                            )
                                        });

                                    state.set(ConductorSubmissionState::OpeningWriter {
                                        run_id: run_id.clone(),
                                    });
                                    if let Err(e) =
                                        open_writer_window(&desktop_id, writer_props).await
                                    {
                                        state.set(ConductorSubmissionState::Failed {
                                            code: "WINDOW_OPEN_FAILED".to_string(),
                                            message: e,
                                        });
                                    } else {
                                        state.set(ConductorSubmissionState::Success {
                                            run_id: run_id.clone(),
                                        });
                                    }
                                    return;
                                }
                                TaskLifecycleDecision::Failed => {
                                    let (code, message) = failure_from_error(run_status.error);
                                    state.set(ConductorSubmissionState::Failed { code, message });
                                    return;
                                }
                                TaskLifecycleDecision::Running => {}
                            },
                            Err(_) => {}
                        }

                        if !opened_writer {
                            match conductor_get_run_state(&run_id).await {
                                Ok(run_state) if run_state_requires_writer(&run_state) => {
                                    state.set(ConductorSubmissionState::OpeningWriter {
                                        run_id: run_id.clone(),
                                    });
                                    let props = Some(writer_props_for_run_document(
                                        &run_state.document_path,
                                        &run_id,
                                    ));
                                    if let Err(e) = open_writer_window(&desktop_id, props).await {
                                        state.set(ConductorSubmissionState::Failed {
                                            code: "WINDOW_OPEN_FAILED".to_string(),
                                            message: e,
                                        });
                                    } else {
                                        state.set(ConductorSubmissionState::Success {
                                            run_id: run_id.clone(),
                                        });
                                        opened_writer = true;
                                    }
                                }
                                _ => {}
                            }
                        }

                        if opened_writer {
                            return;
                        }

                        gloo_timers::future::TimeoutFuture::new(250).await;
                    }

                    match conductor_get_run_status(&run_id).await {
                        Ok(run_status) => {
                            let decision = classify_run_status(run_status.status);
                            match decision {
                                TaskLifecycleDecision::Failed => {
                                    let (code, message) = failure_from_error(run_status.error);
                                    state.set(ConductorSubmissionState::Failed { code, message });
                                }
                                _ => {
                                    state.set(ConductorSubmissionState::Failed {
                                        code: "RUN_STATUS_TIMEOUT".to_string(),
                                        message: format!(
                                            "Run accepted but did not produce toast or writer state in time (last status: {:?})",
                                            run_status.status
                                        ),
                                    });
                                }
                            }
                        }
                        Err(_) => {
                            state.set(ConductorSubmissionState::Failed {
                                code: "RUN_STATUS_TIMEOUT".to_string(),
                                message:
                                    "Run accepted but did not produce toast or writer state in time"
                                        .to_string(),
                            });
                        }
                    }
                }
            }
            TaskLifecycleDecision::Completed => {
                if let Some(toast) = response.toast {
                    state.set(ConductorSubmissionState::ToastReady {
                        run_id: response.run_id,
                        toast,
                    });
                } else {
                    state.set(ConductorSubmissionState::OpeningWriter {
                        run_id: response.run_id.clone(),
                    });

                    if let Err(e) =
                        open_writer_window(&desktop_id, response.writer_window_props).await
                    {
                        state.set(ConductorSubmissionState::Failed {
                            code: "WINDOW_OPEN_FAILED".to_string(),
                            message: e,
                        });
                    } else {
                        state.set(ConductorSubmissionState::Success {
                            run_id: response.run_id,
                        });
                    }
                }
            }
            TaskLifecycleDecision::Failed => {
                let (code, message) = failure_from_error(response.error);
                state.set(ConductorSubmissionState::Failed { code, message });
            }
        }
    }

    async fn open_writer_window(
        desktop_id: &str,
        writer_window_props: Option<WriterWindowProps>,
    ) -> Result<(), String> {
        let props = writer_window_props
            .ok_or_else(|| "No writer_window_props provided in execute response".to_string())?;

        let props_value = serde_json::to_value(&props)
            .map_err(|e| format!("Failed to serialize writer_window_props: {}", e))?;

        match open_window(desktop_id, "writer", "Writer", Some(props_value)).await {
            Ok(_window) => Ok(()),
            Err(e) => Err(format!("Failed to open writer window: {}", e)),
        }
    }

    rsx! {
        // Live telemetry stream overlay (Phase F)
        LiveTelemetryStream {
            state: telemetry_state,
            is_active: conductor_state().is_active(),
        }

        div {
            class: "prompt-bar",
            style: if is_mobile {
                if prompt_expanded() {
                    "display: flex; align-items: center; gap: 0.4rem; padding: 0.5rem 0.5rem; background: var(--promptbar-bg, #111827); border-top: 1px solid var(--border-color, #374151); position: relative; z-index: 2000;"
                } else {
                    "display: flex; align-items: center; gap: 0.35rem; padding: 0.45rem 0.5rem; background: var(--promptbar-bg, #111827); border-top: 1px solid var(--border-color, #374151); position: relative; z-index: 2000;"
                }
            } else {
                "display: flex; align-items: center; gap: 0.5rem; padding: 0.75rem 1rem; background: var(--promptbar-bg, #111827); border-top: 1px solid var(--border-color, #374151); position: relative; z-index: 2000;"
            },

            if is_mobile {
                div {
                    class: "mobile-main",
                    style: "display: flex; align-items: center; gap: 0.25rem; flex: 1; min-width: 0;",

                    if prompt_expanded() {
                        button {
                            class: "prompt-theme-btn",
                            style: "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: var(--window-bg, #1f2937); color: var(--text-secondary, #9ca3af); border: 1px solid var(--border-color, #374151); border-radius: var(--radius-md, 8px); cursor: pointer; flex-shrink: 0;",
                            onclick: move |_| on_toggle_theme.call(()),
                            title: "Toggle theme",
                            if current_theme == "dark" {
                                "☀️"
                            } else {
                                "🌙"
                            }
                        }
                    }

                    if prompt_expanded() {
                        div {
                            class: "prompt-input-container",
                            style: "flex: 1; min-width: 0; position: relative; display: flex; align-items: center;",

                            input {
                                class: "prompt-input",
                                style: "flex: 1; padding: 0.5rem 1rem; background: var(--input-bg, #1f2937); color: var(--text-primary, white); border: 1px solid var(--border-color, #374151); border-radius: var(--radius-md, 8px); font-size: 0.875rem; outline: none; min-width: 0;",
                                placeholder: "Ask anything, paste URL, or type ? for commands...",
                                value: "{input_value}",
                                disabled: conductor_state().is_active(),
                                onfocus: move |_| {
                                    prompt_expanded.set(true);
                                },
                                oninput: move |e| {
                                    input_value.set(e.value().clone());
                                    if matches!(
                                        conductor_state(),
                                        ConductorSubmissionState::Success { .. }
                                            | ConductorSubmissionState::ToastReady { .. }
                                    ) {
                                        conductor_state.set(ConductorSubmissionState::Idle);
                                    }
                                },
                                onkeydown: move |e| {
                                    if e.key() == Key::Enter {
                                        let text = input_value.to_string();
                                        if !text.trim().is_empty() {
                                            handle_conductor_submit.call(text);
                                            input_value.set(String::new());
                                        }
                                    }
                                }
                            }

                            // Conductor state indicator overlay
                            if let Some(display_text) = conductor_state().display_text() {
                                div {
                                    class: "conductor-status-indicator",
                                    style: "position: absolute; right: 0.75rem; display: flex; align-items: center; gap: 0.5rem; padding: 0.25rem 0.75rem; background: var(--accent-bg, #3b82f6); color: white; border-radius: var(--radius-sm, 4px); font-size: 0.75rem; font-weight: 500;",

                                    // Spinner animation
                                    div {
                                        style: "width: 12px; height: 12px; border: 2px solid rgba(255,255,255,0.3); border-top-color: white; border-radius: 50%; animation: spin 1s linear infinite;",
                                    }

                                    span { "{display_text}" }
                                }
                            }

                            // Error message display
                            if let Some(error_msg) = conductor_state().error_message() {
                                div {
                                    class: "conductor-error-indicator",
                                    style: "position: absolute; right: 0.75rem; display: flex; align-items: center; gap: 0.5rem; padding: 0.25rem 0.75rem; background: var(--danger-bg, #ef4444); color: white; border-radius: var(--radius-sm, 4px); font-size: 0.75rem; font-weight: 500; max-width: 60%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                                    title: "{error_msg}",

                                    span { "⚠" }
                                    span { "{error_msg}" }
                                }
                            }

                            if let Some(toast) = conductor_state().toast_payload().cloned() {
                                button {
                                    class: "conductor-toast-indicator",
                                    style: "{toast_style_for_tone(toast.tone)}",
                                    title: "{toast.message}",
                                    onclick: move |_| {
                                        let toast = toast.clone();
                                        let desktop_id = desktop_id_signal.read().clone();
                                        spawn(async move {
                                            if let Some(report_path) = toast.report_path {
                                                let toast_props = WriterWindowProps {
                                                    x: 0,
                                                    y: 0,
                                                    width: 0,
                                                    height: 0,
                                                    path: report_path,
                                                    preview_mode: true,
                                                    run_id: None,
                                                };
                                                if let Err(e) = open_writer_window(&desktop_id, Some(toast_props))
                                                    .await
                                                {
                                                    dioxus_logger::tracing::error!(
                                                        "Failed to open writer from conductor toast: {}",
                                                        e
                                                    );
                                                }
                                            }
                                        });
                                    },
                                    span { "{toast.message}" }
                                }
                            }

                            // Success message display
                            if let Some(success_msg) = conductor_state().success_message() {
                                div {
                                    class: "conductor-success-indicator",
                                    style: "position: absolute; right: 0.75rem; display: flex; align-items: center; gap: 0.5rem; padding: 0.25rem 0.75rem; background: var(--success-bg, #10b981); color: white; border-radius: var(--radius-sm, 4px); font-size: 0.75rem; font-weight: 500;",

                                    span { "✓" }
                                    span { "{success_msg}" }
                                }
                            }
                        }
                    } else {
                        div {
                            class: "mobile-dock-strip",
                            style: "display: flex; align-items: center; gap: 0.25rem; flex: 1; min-width: 0; overflow-x: auto; overflow-y: hidden; padding-right: 0.15rem;",

                            for window in windows.iter() {
                                RunningAppIndicator {
                                    key: "mobile-strip-{window.id}",
                                    window: window.clone(),
                                    is_active: active_window.as_ref() == Some(&window.id),
                                    on_focus: on_focus_window,
                                }
                            }
                        }

                        ShowDesktopIndicator {
                            on_show_desktop,
                        }
                    }
                }
            } else {
                button {
                    class: "prompt-theme-btn",
                    style: "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: var(--window-bg, #1f2937); color: var(--text-secondary, #9ca3af); border: 1px solid var(--border-color, #374151); border-radius: var(--radius-md, 8px); cursor: pointer; flex-shrink: 0;",
                    onclick: move |_| on_toggle_theme.call(()),
                    title: "Toggle theme",
                    if current_theme == "dark" {
                        "☀️"
                    } else {
                        "🌙"
                    }
                }

                div {
                    class: "prompt-input-container",
                    style: "flex: 1; min-width: 0; position: relative; display: flex; align-items: center;",

                    input {
                        class: "prompt-input",
                        style: "flex: 1; padding: 0.5rem 1rem; background: var(--input-bg, #1f2937); color: var(--text-primary, white); border: 1px solid var(--border-color, #374151); border-radius: var(--radius-md, 8px); font-size: 0.875rem; outline: none; min-width: 0;",
                        placeholder: "Ask anything, paste URL, or type ? for commands...",
                        value: "{input_value}",
                        disabled: conductor_state().is_active(),
                        oninput: move |e| {
                            input_value.set(e.value().clone());
                            if matches!(
                                conductor_state(),
                                ConductorSubmissionState::Success { .. }
                                    | ConductorSubmissionState::ToastReady { .. }
                            ) {
                                conductor_state.set(ConductorSubmissionState::Idle);
                            }
                        },
                        onkeydown: move |e| {
                            if e.key() == Key::Enter {
                                let text = input_value.to_string();
                                if !text.trim().is_empty() {
                                    handle_conductor_submit.call(text);
                                    input_value.set(String::new());
                                }
                            }
                        }
                    }

                    // Conductor state indicator overlay
                    if let Some(display_text) = conductor_state().display_text() {
                        div {
                            class: "conductor-status-indicator",
                            style: "position: absolute; right: 0.75rem; display: flex; align-items: center; gap: 0.5rem; padding: 0.25rem 0.75rem; background: var(--accent-bg, #3b82f6); color: white; border-radius: var(--radius-sm, 4px); font-size: 0.75rem; font-weight: 500;",

                            // Spinner animation
                            div {
                                style: "width: 12px; height: 12px; border: 2px solid rgba(255,255,255,0.3); border-top-color: white; border-radius: 50%; animation: spin 1s linear infinite;",
                            }

                            span { "{display_text}" }
                        }
                    }

                    // Error message display
                    if let Some(error_msg) = conductor_state().error_message() {
                        div {
                            class: "conductor-error-indicator",
                            style: "position: absolute; right: 0.75rem; display: flex; align-items: center; gap: 0.5rem; padding: 0.25rem 0.75rem; background: var(--danger-bg, #ef4444); color: white; border-radius: var(--radius-sm, 4px); font-size: 0.75rem; font-weight: 500; max-width: 60%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                            title: "{error_msg}",

                            span { "⚠" }
                            span { "{error_msg}" }
                        }
                    }

                    if let Some(toast) = conductor_state().toast_payload().cloned() {
                        button {
                            class: "conductor-toast-indicator",
                            style: "{toast_style_for_tone(toast.tone)}",
                            title: "{toast.message}",
                            onclick: move |_| {
                                let toast = toast.clone();
                                let desktop_id = desktop_id_signal.read().clone();
                                spawn(async move {
                                    if let Some(report_path) = toast.report_path {
                                        let toast_props = WriterWindowProps {
                                            x: 0,
                                            y: 0,
                                            width: 0,
                                            height: 0,
                                            path: report_path,
                                            preview_mode: true,
                                            run_id: None,
                                        };
                                        if let Err(e) = open_writer_window(&desktop_id, Some(toast_props))
                                            .await
                                        {
                                            dioxus_logger::tracing::error!(
                                                "Failed to open writer from conductor toast: {}",
                                                e
                                            );
                                        }
                                    }
                                });
                            },
                            span { "{toast.message}" }
                        }
                    }

                    // Success message display
                    if let Some(success_msg) = conductor_state().success_message() {
                        div {
                            class: "conductor-success-indicator",
                            style: "position: absolute; right: 0.75rem; display: flex; align-items: center; gap: 0.5rem; padding: 0.25rem 0.75rem; background: var(--success-bg, #10b981); color: white; border-radius: var(--radius-sm, 4px); font-size: 0.75rem; font-weight: 500;",

                            span { "✓" }
                            span { "{success_msg}" }
                        }
                    }
                }
            }

            if !is_mobile {
                div {
                    class: "running-apps",
                    style: "display: flex; align-items: center; gap: 0.25rem; flex-shrink: 0;",

                    ShowDesktopIndicator {
                        on_show_desktop,
                    }

                    for window in windows.iter() {
                        RunningAppIndicator {
                            key: "{window.id}",
                            window: window.clone(),
                            is_active: active_window.as_ref() == Some(&window.id),
                            on_focus: on_focus_window,
                        }
                    }
                }
            }

            if is_mobile {
                div {
                    class: "mobile-dock",
                    style: "display: flex; align-items: center; gap: 0.25rem; flex-shrink: 0;",

                    button {
                        class: "mobile-mode-toggle",
                        style: if prompt_expanded() {
                            "position: relative; z-index: 2100; width: 40px; height: 32px; display: flex; align-items: center; justify-content: center; background: var(--accent-bg, #3b82f6); color: white; border: none; border-radius: var(--radius-md, 8px); cursor: pointer; font-size: 0.75rem; font-weight: 700; box-shadow: var(--shadow-sm, 0 1px 2px rgba(0, 0, 0, 0.3));"
                        } else {
                            "position: relative; z-index: 2100; width: 40px; height: 32px; display: flex; align-items: center; justify-content: center; background: var(--window-bg, #1f2937); color: var(--text-secondary, #9ca3af); border: 1px solid var(--border-color, #374151); border-radius: var(--radius-md, 8px); cursor: pointer; font-size: 0.75rem; font-weight: 700; box-shadow: var(--shadow-sm, 0 1px 2px rgba(0, 0, 0, 0.3));"
                        },
                        onclick: move |_| {
                            prompt_expanded.set(!prompt_expanded());
                        },
                        title: if prompt_expanded() { "Show windows tray" } else { "Show prompt" },
                        "{windows.len()}"
                    }

                    div {
                        style: if connected {
                            "display: flex; align-items: center; justify-content: center; width: 18px; height: 18px; color: #10b981; font-size: 0.8rem;"
                        } else {
                            "display: flex; align-items: center; justify-content: center; width: 18px; height: 18px; color: #f59e0b; font-size: 0.8rem;"
                        },
                        if connected { "●" } else { "◐" }
                    }
                }
            } else {
                div {
                    class: if connected { "ws-status connected" } else { "ws-status" },
                    style: if connected {
                        "display: flex; align-items: center; gap: 0.25rem; padding: 0.25rem 0.5rem; background: var(--success-bg, #10b981); color: white; border-radius: var(--radius-sm, 4px); font-size: 0.75rem; flex-shrink: 0;"
                    } else {
                        "display: flex; align-items: center; gap: 0.25rem; padding: 0.25rem 0.5rem; background: var(--warning-bg, #f59e0b); color: white; border-radius: var(--radius-sm, 4px); font-size: 0.75rem; flex-shrink: 0;"
                    },

                    span { if connected { "●" } else { "◐" } }
                    span { if connected { "Connected" } else { "Connecting..." } }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_run_status_failed_on_error() {
        assert_eq!(
            classify_run_status(ConductorRunStatus::Failed),
            TaskLifecycleDecision::Failed
        );
        assert_eq!(
            classify_run_status(ConductorRunStatus::Blocked),
            TaskLifecycleDecision::Failed
        );
    }

    #[test]
    fn classify_run_status_running_for_non_terminal_statuses() {
        assert_eq!(
            classify_run_status(ConductorRunStatus::Initializing),
            TaskLifecycleDecision::Running
        );
        assert_eq!(
            classify_run_status(ConductorRunStatus::Running),
            TaskLifecycleDecision::Running
        );
        assert_eq!(
            classify_run_status(ConductorRunStatus::WaitingForCalls),
            TaskLifecycleDecision::Running
        );
        assert_eq!(
            classify_run_status(ConductorRunStatus::Completing),
            TaskLifecycleDecision::Running
        );
    }

    #[test]
    fn classify_run_status_completed_for_completed_status() {
        assert_eq!(
            classify_run_status(ConductorRunStatus::Completed),
            TaskLifecycleDecision::Completed
        );
    }

    #[test]
    fn failure_from_error_uses_typed_backend_error() {
        let (code, message) = failure_from_error(Some(ConductorError {
            code: "EXEC_FAIL".to_string(),
            message: "Typed failure".to_string(),
            failure_kind: None,
        }));
        assert_eq!(code, "EXEC_FAIL");
        assert_eq!(message, "Typed failure");
    }

    #[test]
    fn run_state_requires_writer_is_false_for_immediate_only_agenda() {
        let now = chrono::Utc::now();
        let run = shared_types::ConductorRunState {
            run_id: "run-1".to_string(),
            objective: "hi".to_string(),
            status: ConductorRunStatus::Running,
            created_at: now,
            updated_at: now,
            completed_at: None,
            agenda: vec![shared_types::ConductorAgendaItem {
                item_id: "item-1".to_string(),
                capability: "immediate_response".to_string(),
                objective: "hi".to_string(),
                priority: 0,
                depends_on: vec![],
                status: shared_types::AgendaItemStatus::Ready,
                created_at: now,
                started_at: None,
                completed_at: None,
            }],
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path: "conductor/runs/run-1/draft.md".to_string(),
            output_mode: ConductorOutputMode::Auto,
            desktop_id: "desktop-1".to_string(),
        };

        assert!(!run_state_requires_writer(&run));
    }

    #[test]
    fn run_state_requires_writer_is_true_for_writer_capability() {
        let now = chrono::Utc::now();
        let run = shared_types::ConductorRunState {
            run_id: "run-2".to_string(),
            objective: "summarize".to_string(),
            status: ConductorRunStatus::Running,
            created_at: now,
            updated_at: now,
            completed_at: None,
            agenda: vec![shared_types::ConductorAgendaItem {
                item_id: "item-1".to_string(),
                capability: "writer".to_string(),
                objective: "summarize".to_string(),
                priority: 0,
                depends_on: vec![],
                status: shared_types::AgendaItemStatus::Ready,
                created_at: now,
                started_at: None,
                completed_at: None,
            }],
            active_calls: vec![],
            artifacts: vec![],
            decision_log: vec![],
            document_path: "conductor/runs/run-2/draft.md".to_string(),
            output_mode: ConductorOutputMode::Auto,
            desktop_id: "desktop-1".to_string(),
        };

        assert!(run_state_requires_writer(&run));
    }
}

#[component]
pub fn ShowDesktopIndicator(on_show_desktop: Callback<()>) -> Element {
    rsx! {
        button {
            class: "show-desktop",
            style: "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: var(--window-bg, #1f2937); color: var(--text-secondary, #9ca3af); border: 1px solid var(--border-color, #374151); border-radius: var(--radius-md, 8px); cursor: pointer; font-size: 1rem;",
            onclick: move |_| on_show_desktop.call(()),
            title: "Show desktop",
            "⌂"
        }
    }
}

#[component]
pub fn RunningAppIndicator(
    window: WindowState,
    is_active: bool,
    on_focus: Callback<String>,
) -> Element {
    let icon = get_app_icon(&window.app_id);
    let window_id = window.id.clone();

    rsx! {
        button {
            class: if is_active { "running-app active" } else { "running-app" },
            style: if is_active {
                "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: var(--accent-bg, #3b82f6); color: white; border: none; border-radius: var(--radius-md, 8px); cursor: pointer; font-size: 1.25rem;"
            } else {
                "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: var(--window-bg, #1f2937); color: var(--text-secondary, #9ca3af); border: 1px solid var(--border-color, #374151); border-radius: var(--radius-md, 8px); cursor: pointer; font-size: 1.25rem;"
            },
            onclick: move |_| on_focus.call(window_id.clone()),
            title: "{window.title}",
            "{icon}"
        }
    }
}
