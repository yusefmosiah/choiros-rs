use dioxus::prelude::*;
use shared_types::{
    ConductorError, ConductorExecuteResponse, ConductorOutputMode, ConductorRunStatus,
    ConductorToastPayload, EventImportance, WindowState, WriterWindowProps,
};

use crate::api::{execute_conductor, open_window};
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
}

impl TelemetryStreamState {
    pub fn new(max_lines: usize) -> Self {
        Self {
            lines: Vec::new(),
            max_lines,
            enabled: true,
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
        let line = TelemetryLine {
            id: format!("{}-{}", capability, now_ms),
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

    pub fn clear(&mut self) {
        self.lines.clear();
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
                    key: "{line.1.id}",
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
                        "[{line.1.capability}]"
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
                state.set(ConductorSubmissionState::OpeningWriter {
                    run_id: response.run_id.clone(),
                });

                if let Err(e) = open_writer_window(&desktop_id, response.writer_window_props).await
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
                                    style: "position: absolute; right: 0.75rem; display: flex; align-items: center; gap: 0.5rem; padding: 0.35rem 0.75rem; background: #14b8a6; color: #042f2e; border: 1px solid #2dd4bf; border-radius: var(--radius-sm, 4px); font-size: 0.75rem; font-weight: 600; max-width: 70%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; cursor: pointer;",
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
                                    span { "✓" }
                                    span { "{toast.title}: {toast.message}" }
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
                            style: "position: absolute; right: 0.75rem; display: flex; align-items: center; gap: 0.5rem; padding: 0.35rem 0.75rem; background: #14b8a6; color: #042f2e; border: 1px solid #2dd4bf; border-radius: var(--radius-sm, 4px); font-size: 0.75rem; font-weight: 600; max-width: 70%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; cursor: pointer;",
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
                            span { "✓" }
                            span { "{toast.title}: {toast.message}" }
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
