use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use shared_types::{
    ConductorError, ConductorExecuteResponse, ConductorOutputMode, ConductorTaskStatus,
    ConductorToastPayload, EventImportance, WindowState,
};

use crate::api::{execute_conductor, open_window, poll_conductor_task};
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

    pub fn add_line(&mut self, message: String, capability: String, phase: String, importance: EventImportance) {
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
            let tool = payload.get("tool").and_then(|v| v.as_str()).unwrap_or("unknown");
            format!("Calling {}...", tool)
        }
        "conductor.tool.result" => {
            let tool = payload.get("tool").and_then(|v| v.as_str()).unwrap_or("unknown");
            let success = payload.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
            if success {
                format!("{} completed", tool)
            } else {
                format!("{} failed", tool)
            }
        }
        "conductor.progress" => {
            payload
                .get("message")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| "Processing...".to_string())
        }
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

/// State machine for conductor submission flow
#[derive(Clone, Debug, PartialEq)]
pub enum ConductorSubmissionState {
    /// Idle state - ready for new submission
    Idle,
    /// Initial submission in progress
    Submitting,
    /// Task is running (queued/running/waiting_worker) with current status
    Running {
        task_id: String,
        status: ConductorTaskStatus,
    },
    /// Task completed, opening writer
    OpeningWriter { task_id: String },
    /// Task succeeded and writer opened
    Success { task_id: String },
    /// Task succeeded with prompt-bar toast output.
    ToastReady {
        task_id: String,
        toast: ConductorToastPayload,
    },
    /// Task failed with typed error
    Failed { code: String, message: String },
}

impl ConductorSubmissionState {
    /// Get display text for the current state
    pub fn display_text(&self) -> Option<&'static str> {
        match self {
            ConductorSubmissionState::Idle => None,
            ConductorSubmissionState::Submitting => Some("Submitting..."),
            ConductorSubmissionState::Running { status, .. } => match status {
                ConductorTaskStatus::Queued => Some("Queued..."),
                ConductorTaskStatus::Running => Some("Running..."),
                ConductorTaskStatus::WaitingWorker => Some("Waiting for worker..."),
                _ => Some("Processing..."),
            },
            ConductorSubmissionState::OpeningWriter { .. } => Some("Opening Writer..."),
            ConductorSubmissionState::Success { .. } => None,
            ConductorSubmissionState::ToastReady { .. } => None,
            ConductorSubmissionState::Failed { .. } => None,
        }
    }

    /// Check if the state represents an active operation
    pub fn is_active(&self) -> bool {
        !matches!(
            self,
            ConductorSubmissionState::Idle
                | ConductorSubmissionState::Success { .. }
                | ConductorSubmissionState::ToastReady { .. }
                | ConductorSubmissionState::Failed { .. }
        )
    }

    /// Get error message if failed
    pub fn error_message(&self) -> Option<String> {
        match self {
            ConductorSubmissionState::Failed { code, message } => {
                Some(format!("{}: {}", code, message))
            }
            _ => None,
        }
    }

    /// Get success message if writer opened.
    pub fn success_message(&self) -> Option<&'static str> {
        match self {
            ConductorSubmissionState::Success { .. } => Some("Writer opened"),
            _ => None,
        }
    }

    /// Get toast payload if the run completed with toast output.
    pub fn toast_payload(&self) -> Option<&ConductorToastPayload> {
        match self {
            ConductorSubmissionState::ToastReady { toast, .. } => Some(toast),
            _ => None,
        }
    }
}

/// Poll interval in milliseconds
const POLL_INTERVAL_MS: u32 = 1000;
/// Maximum number of poll attempts before giving up
const MAX_POLL_ATTEMPTS: u32 = 300; // 5 minutes at 1 second intervals

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskLifecycleDecision {
    InProgress(ConductorTaskStatus),
    Completed,
    Failed,
}

fn classify_task_status(status: ConductorTaskStatus) -> TaskLifecycleDecision {
    match status {
        ConductorTaskStatus::Completed => TaskLifecycleDecision::Completed,
        ConductorTaskStatus::Failed => TaskLifecycleDecision::Failed,
        ConductorTaskStatus::Queued
        | ConductorTaskStatus::Running
        | ConductorTaskStatus::WaitingWorker => TaskLifecycleDecision::InProgress(status),
    }
}

fn failure_from_error(error: Option<ConductorError>) -> (String, String) {
    error.map(|e| (e.code, e.message)).unwrap_or_else(|| {
        (
            "TASK_FAILED".to_string(),
            "Task failed without error details".to_string(),
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
    let mut mobile_dock_expanded = use_signal(|| false);
    let visible_mobile_icons = 2usize;
    let mut conductor_state = use_signal(|| ConductorSubmissionState::Idle);
    let desktop_id_signal = use_signal(|| desktop_id.clone());

    // Conductor submission handler
    let handle_conductor_submit = use_callback(move |objective: String| {
        let desktop_id = desktop_id_signal.read().clone();
        let mut state = conductor_state.clone();

        if state().is_active() {
            return;
        }
        state.set(ConductorSubmissionState::Submitting);

        spawn(async move {
            // Execute conductor task
            match execute_conductor(
                &objective,
                &desktop_id,
                ConductorOutputMode::Auto,
                None,
            )
            .await
            {
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

    // Handle the conductor execute response and start polling if needed
    async fn handle_conductor_response(
        response: ConductorExecuteResponse,
        mut state: Signal<ConductorSubmissionState>,
        desktop_id: String,
    ) {
        match classify_task_status(response.status) {
            TaskLifecycleDecision::Completed => {
                if let Some(toast) = response.toast {
                    state.set(ConductorSubmissionState::ToastReady {
                        task_id: response.task_id,
                        toast,
                    });
                } else {
                    // Task completed immediately, open writer.
                    state.set(ConductorSubmissionState::OpeningWriter {
                        task_id: response.task_id.clone(),
                    });

                    if let Err(e) = open_writer_window(
                        &desktop_id,
                        response.report_path,
                        response.writer_window_props,
                    )
                    .await
                    {
                        state.set(ConductorSubmissionState::Failed {
                            code: "WINDOW_OPEN_FAILED".to_string(),
                            message: e,
                        });
                    } else {
                        state.set(ConductorSubmissionState::Success {
                            task_id: response.task_id,
                        });
                    }
                }
            }
            TaskLifecycleDecision::Failed => {
                // Task failed immediately
                let (code, message) = failure_from_error(response.error);
                state.set(ConductorSubmissionState::Failed { code, message });
            }
            TaskLifecycleDecision::InProgress(status) => {
                // Task is in progress, start polling
                state.set(ConductorSubmissionState::Running {
                    task_id: response.task_id.clone(),
                    status,
                });

                // Start polling loop
                poll_conductor_task_until_complete(
                    response.task_id,
                    state,
                    desktop_id,
                    response.writer_window_props,
                )
                .await;
            }
        }
    }

    // Poll conductor task until it reaches completed or failed state
    async fn poll_conductor_task_until_complete(
        task_id: String,
        mut state: Signal<ConductorSubmissionState>,
        desktop_id: String,
        fallback_writer_window_props: Option<serde_json::Value>,
    ) {
        let mut attempts = 0u32;

        loop {
            if attempts >= MAX_POLL_ATTEMPTS {
                state.set(ConductorSubmissionState::Failed {
                    code: "POLL_TIMEOUT".to_string(),
                    message: format!("Task polling exceeded {} attempts", MAX_POLL_ATTEMPTS),
                });
                return;
            }

            // Wait before polling
            TimeoutFuture::new(POLL_INTERVAL_MS).await;

            match poll_conductor_task(&task_id).await {
                Ok(task_state) => {
                    match classify_task_status(task_state.status) {
                        TaskLifecycleDecision::Completed => {
                            if let Some(toast) = task_state.toast {
                                state.set(ConductorSubmissionState::ToastReady {
                                    task_id: task_id.clone(),
                                    toast,
                                });
                            } else {
                                state.set(ConductorSubmissionState::OpeningWriter {
                                    task_id: task_id.clone(),
                                });

                                if let Err(e) = open_writer_window(
                                    &desktop_id,
                                    task_state.report_path,
                                    fallback_writer_window_props.clone(),
                                )
                                .await
                                {
                                    state.set(ConductorSubmissionState::Failed {
                                        code: "WINDOW_OPEN_FAILED".to_string(),
                                        message: e,
                                    });
                                } else {
                                    state.set(ConductorSubmissionState::Success {
                                        task_id: task_id.clone(),
                                    });
                                }
                            }
                            return;
                        }
                        TaskLifecycleDecision::Failed => {
                            let (code, message) = failure_from_error(task_state.error);
                            state.set(ConductorSubmissionState::Failed { code, message });
                            return;
                        }
                        TaskLifecycleDecision::InProgress(status) => {
                            // Update running state with current status
                            state.set(ConductorSubmissionState::Running {
                                task_id: task_id.clone(),
                                status,
                            });
                        }
                    }
                }
                Err(e) => {
                    state.set(ConductorSubmissionState::Failed {
                        code: "POLL_FAILED".to_string(),
                        message: e,
                    });
                    return;
                }
            }

            attempts += 1;
        }
    }

    // Open writer window with the report
    async fn open_writer_window(
        desktop_id: &str,
        report_path: Option<String>,
        writer_window_props: Option<serde_json::Value>,
    ) -> Result<(), String> {
        // Normalize writer props to current Writer contract (`path`) while
        // accepting legacy producer key (`file_path`) for compatibility.
        let props = if let Some(mut props) = writer_window_props {
            if let Some(obj) = props.as_object_mut() {
                if !obj.contains_key("path") {
                    if let Some(file_path) = obj.get("file_path").cloned() {
                        obj.insert("path".to_string(), file_path);
                    }
                }
            }
            props
        } else if let Some(path) = report_path {
            serde_json::json!({
                "path": path,
                "preview_mode": true,
            })
        } else {
            return Err("No report path or window props provided".to_string());
        };

        match open_window(desktop_id, "writer", "Writer", Some(props)).await {
            Ok(_window) => {
                // Window opened successfully
                Ok(())
            }
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
            style: "display: flex; align-items: center; gap: 0.5rem; padding: 0.75rem 1rem; background: var(--promptbar-bg, #111827); border-top: 1px solid var(--border-color, #374151); position: relative; z-index: 2000;",

            button {
                class: "prompt-help-btn",
                style: "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: var(--accent-bg, #3b82f6); color: white; border: none; border-radius: var(--radius-md, 8px); cursor: pointer; font-weight: 600; flex-shrink: 0;",
                onclick: move |_| {},
                "?"
            }

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
                style: "flex: 1; position: relative; display: flex; align-items: center;",

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
                                    if let Err(e) = open_writer_window(
                                        &desktop_id,
                                        Some(report_path),
                                        None,
                                    )
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
                    style: "display: flex; align-items: center; gap: 0.25rem; flex-shrink: 0; margin-left: auto;",

                    ShowDesktopIndicator {
                        on_show_desktop,
                    }

                    for window in windows.iter().take(visible_mobile_icons) {
                        RunningAppIndicator {
                            key: "{window.id}",
                            window: window.clone(),
                            is_active: active_window.as_ref() == Some(&window.id),
                            on_focus: on_focus_window,
                        }
                    }

                    if windows.len() > visible_mobile_icons {
                        button {
                            class: "mobile-dock-more",
                            style: "position: relative; z-index: 2100; width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: var(--window-bg, #1f2937); color: var(--text-secondary, #9ca3af); border: 1px solid var(--border-color, #374151); border-radius: var(--radius-md, 8px); cursor: pointer; font-size: 0.75rem; font-weight: 600;",
                            onclick: move |_| mobile_dock_expanded.set(!mobile_dock_expanded()),
                            title: "Show all open windows",
                            "+{windows.len() - visible_mobile_icons}"
                        }
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

                if mobile_dock_expanded() && windows.len() > visible_mobile_icons {
                    div {
                        class: "mobile-dock-panel",
                        style: "position: absolute; right: 0.75rem; bottom: 3.5rem; z-index: 2200; display: flex; flex-wrap: wrap; gap: 0.35rem; max-width: min(80vw, 320px); padding: 0.5rem; background: var(--window-bg, #1f2937); border: 1px solid var(--border-color, #374151); border-radius: var(--radius-md, 8px); box-shadow: var(--shadow-lg, 0 10px 40px rgba(0,0,0,0.5));",
                        for window in windows.iter().skip(visible_mobile_icons) {
                            RunningAppIndicator {
                                key: "mobile-overflow-{window.id}",
                                window: window.clone(),
                                is_active: active_window.as_ref() == Some(&window.id),
                                on_focus: on_focus_window,
                            }
                        }
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
    fn classify_task_status_maps_all_typed_states() {
        assert_eq!(
            classify_task_status(ConductorTaskStatus::Queued),
            TaskLifecycleDecision::InProgress(ConductorTaskStatus::Queued)
        );
        assert_eq!(
            classify_task_status(ConductorTaskStatus::Running),
            TaskLifecycleDecision::InProgress(ConductorTaskStatus::Running)
        );
        assert_eq!(
            classify_task_status(ConductorTaskStatus::WaitingWorker),
            TaskLifecycleDecision::InProgress(ConductorTaskStatus::WaitingWorker)
        );
        assert_eq!(
            classify_task_status(ConductorTaskStatus::Completed),
            TaskLifecycleDecision::Completed
        );
        assert_eq!(
            classify_task_status(ConductorTaskStatus::Failed),
            TaskLifecycleDecision::Failed
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
