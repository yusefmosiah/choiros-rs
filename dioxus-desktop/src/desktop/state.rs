use dioxus::prelude::{Signal, WritableExt};
use shared_types::{
    AppDefinition, ChangesetImpact, DesktopState, PatchOp, PatchSource, WindowState,
    WriterRunStatusKind,
};

use crate::desktop::ws::WsEvent;

#[derive(Debug, Clone, PartialEq)]
pub struct PendingPatch {
    pub patch_id: String,
    pub revision: u64,
    pub source: PatchSource,
    pub ops: Vec<PatchOp>,
    pub proposal: Option<String>,
    pub base_version_id: Option<u64>,
    pub target_version_id: Option<u64>,
    pub overlay_id: Option<String>,
    pub applied: bool,
}

/// A received changeset summary (from writer.run.changeset events)
#[derive(Debug, Clone, PartialEq)]
pub struct LiveChangeset {
    pub patch_id: String,
    pub loop_id: Option<String>,
    pub summary: String,
    pub impact: ChangesetImpact,
    pub op_taxonomy: Vec<String>,
}

/// Active writer run state for live patch tracking
#[derive(Debug, Clone)]
pub struct ActiveWriterRun {
    pub run_id: String,
    pub document_path: String,
    pub revision: u64,
    pub status: WriterRunStatusKind,
    pub objective: Option<String>,
    pub phase: Option<String>,
    pub message: Option<String>,
    pub progress_pct: Option<u8>,
    pub source_refs: Vec<String>,
    pub proposal: Option<String>,
    pub pending_patches: Vec<PendingPatch>,
    pub last_applied_revision: u64,
    /// Recent changeset summaries from writer.run.changeset events (capped at 20)
    pub recent_changesets: Vec<LiveChangeset>,
}

impl Default for ActiveWriterRun {
    fn default() -> Self {
        Self {
            run_id: String::new(),
            document_path: String::new(),
            revision: 0,
            status: WriterRunStatusKind::Initializing,
            objective: None,
            phase: None,
            message: None,
            progress_pct: None,
            source_refs: Vec::new(),
            proposal: None,
            pending_patches: Vec::new(),
            last_applied_revision: 0,
            recent_changesets: Vec::new(),
        }
    }
}

/// Global signal for active writer runs, keyed by document_path
pub static ACTIVE_WRITER_RUNS: dioxus::signals::GlobalSignal<
    std::collections::HashMap<String, ActiveWriterRun>,
> = dioxus::signals::GlobalSignal::new(std::collections::HashMap::new);

fn merge_source_refs(run: &mut ActiveWriterRun, incoming: &[String], prioritize: bool) {
    if incoming.is_empty() {
        return;
    }

    let mut normalized = Vec::new();
    for source in incoming {
        let trimmed = source.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !normalized
            .iter()
            .any(|existing: &String| existing == trimmed)
        {
            normalized.push(trimmed.to_string());
        }
    }
    if normalized.is_empty() {
        return;
    }

    if prioritize {
        for source in normalized.iter().rev() {
            if let Some(existing_idx) = run.source_refs.iter().position(|v| v == source) {
                run.source_refs.remove(existing_idx);
            }
            run.source_refs.insert(0, source.clone());
        }
    } else {
        for source in normalized {
            if !run.source_refs.iter().any(|existing| existing == &source) {
                run.source_refs.push(source);
            }
        }
    }
}

/// Update the global writer runs state from a WsEvent
pub fn update_writer_runs_from_event(event: &WsEvent) {
    let mut runs = ACTIVE_WRITER_RUNS.write();
    apply_writer_runs_event(&mut runs, event);
}

pub fn apply_writer_runs_event(
    runs: &mut std::collections::HashMap<String, ActiveWriterRun>,
    event: &WsEvent,
) {
    match event {
        WsEvent::WriterRunPatch { base, payload } => {
            let patch = PendingPatch {
                patch_id: payload.patch_id.clone(),
                revision: base.revision,
                source: payload.source.clone(),
                ops: payload.ops.clone(),
                proposal: payload.proposal.clone(),
                base_version_id: payload.base_version_id,
                target_version_id: payload.target_version_id,
                overlay_id: payload.overlay_id.clone(),
                applied: false,
            };
            if let Some(existing) = runs.get_mut(&base.document_path) {
                existing.revision = base.revision;
                existing.proposal = payload.proposal.clone();
                existing.pending_patches.push(patch);
            } else {
                let run = ActiveWriterRun {
                    run_id: base.run_id.clone(),
                    document_path: base.document_path.clone(),
                    revision: base.revision,
                    status: WriterRunStatusKind::Running,
                    objective: None,
                    phase: None,
                    message: None,
                    progress_pct: None,
                    source_refs: Vec::new(),
                    proposal: payload.proposal.clone(),
                    pending_patches: vec![patch],
                    last_applied_revision: 0,
                    recent_changesets: Vec::new(),
                };
                runs.insert(base.document_path.clone(), run);
            }
        }
        WsEvent::WriterRunStarted { base, objective } => {
            let run = ActiveWriterRun {
                run_id: base.run_id.clone(),
                document_path: base.document_path.clone(),
                revision: base.revision,
                status: WriterRunStatusKind::Initializing,
                objective: Some(objective.clone()),
                phase: None,
                message: None,
                progress_pct: None,
                source_refs: Vec::new(),
                proposal: None,
                pending_patches: Vec::new(),
                last_applied_revision: 0,
                recent_changesets: Vec::new(),
            };
            runs.insert(base.document_path.clone(), run);
        }
        WsEvent::WriterRunProgress {
            base,
            phase,
            message,
            progress_pct,
            source_refs,
        } => {
            if let Some(run) = runs.get_mut(&base.document_path) {
                run.revision = base.revision;
                if !matches!(
                    run.status,
                    WriterRunStatusKind::Completed
                        | WriterRunStatusKind::Failed
                        | WriterRunStatusKind::Blocked
                ) {
                    run.status = WriterRunStatusKind::Running;
                }
                run.phase = Some(phase.clone());
                run.message = Some(message.clone());
                run.progress_pct = *progress_pct;
                let prioritize = phase.contains("source_refs");
                merge_source_refs(run, source_refs, prioritize);
            } else {
                runs.insert(
                    base.document_path.clone(),
                    ActiveWriterRun {
                        run_id: base.run_id.clone(),
                        document_path: base.document_path.clone(),
                        revision: base.revision,
                        status: WriterRunStatusKind::Running,
                        objective: None,
                        phase: Some(phase.clone()),
                        message: Some(message.clone()),
                        progress_pct: *progress_pct,
                        source_refs: source_refs.clone(),
                        proposal: None,
                        pending_patches: Vec::new(),
                        last_applied_revision: 0,
                        recent_changesets: Vec::new(),
                    },
                );
            }
        }
        WsEvent::WriterRunStatus {
            base,
            status,
            message,
        } => {
            if let Some(run) = runs.get_mut(&base.document_path) {
                run.revision = base.revision;
                run.status = *status;
                if let Some(msg) = message {
                    run.message = Some(msg.clone());
                }
            } else {
                runs.insert(
                    base.document_path.clone(),
                    ActiveWriterRun {
                        run_id: base.run_id.clone(),
                        document_path: base.document_path.clone(),
                        revision: base.revision,
                        status: *status,
                        objective: None,
                        phase: None,
                        message: message.clone(),
                        progress_pct: None,
                        source_refs: Vec::new(),
                        proposal: None,
                        pending_patches: Vec::new(),
                        last_applied_revision: 0,
                        recent_changesets: Vec::new(),
                    },
                );
            }
        }
        WsEvent::WriterRunFailed {
            base,
            error_code: _,
            error_message,
            failure_kind: _,
        } => {
            if let Some(run) = runs.get_mut(&base.document_path) {
                run.revision = base.revision;
                run.status = WriterRunStatusKind::Failed;
                run.message = Some(error_message.clone());
            } else {
                runs.insert(
                    base.document_path.clone(),
                    ActiveWriterRun {
                        run_id: base.run_id.clone(),
                        document_path: base.document_path.clone(),
                        revision: base.revision,
                        status: WriterRunStatusKind::Failed,
                        objective: None,
                        phase: None,
                        message: Some(error_message.clone()),
                        progress_pct: None,
                        source_refs: Vec::new(),
                        proposal: None,
                        pending_patches: Vec::new(),
                        last_applied_revision: 0,
                        recent_changesets: Vec::new(),
                    },
                );
            }
        }
        WsEvent::WriterRunChangeset {
            base,
            patch_id,
            loop_id,
            summary,
            impact,
            op_taxonomy,
        } => {
            let entry = LiveChangeset {
                patch_id: patch_id.clone(),
                loop_id: loop_id.clone(),
                summary: summary.clone(),
                impact: impact.clone(),
                op_taxonomy: op_taxonomy.clone(),
            };
            // Match by document_path if available, otherwise fall back to run_id scan.
            let matched = if !base.document_path.is_empty() {
                runs.get_mut(&base.document_path)
            } else {
                runs.values_mut().find(|r| r.run_id == base.run_id)
            };
            if let Some(run) = matched {
                run.recent_changesets.push(entry);
                // Cap to 20 most recent
                if run.recent_changesets.len() > 20 {
                    run.recent_changesets.remove(0);
                }
            } else if !base.run_id.is_empty() {
                let mut new_run = ActiveWriterRun {
                    run_id: base.run_id.clone(),
                    document_path: base.document_path.clone(),
                    revision: base.revision,
                    ..Default::default()
                };
                new_run.recent_changesets.push(entry);
                // Key by run_id when document_path is unavailable
                let key = if base.document_path.is_empty() {
                    base.run_id.clone()
                } else {
                    base.document_path.clone()
                };
                runs.insert(key, new_run);
            }
        }
        _ => {}
    }
}

pub fn apply_ws_event(
    event: WsEvent,
    desktop_state: &mut Signal<Option<DesktopState>>,
    ws_connected: &mut Signal<bool>,
) {
    match event {
        WsEvent::Connected => {
            ws_connected.set(true);
        }
        WsEvent::Disconnected => {
            ws_connected.set(false);
        }
        _ => {
            let next_state = {
                let current = desktop_state.write().take();
                apply_desktop_state_event(current, &event)
            };
            desktop_state.set(next_state);
        }
    }
}

pub fn apply_desktop_state_event(
    state: Option<DesktopState>,
    event: &WsEvent,
) -> Option<DesktopState> {
    match event {
        WsEvent::DesktopStateUpdate(state) => Some(state.clone()),
        WsEvent::AppRegistered(app) => {
            let mut state = state?;
            upsert_app(&mut state, app.clone());
            Some(state)
        }
        WsEvent::WindowOpened(window) => {
            let mut state = state?;
            push_window_and_activate(&mut state, window.clone());
            Some(state)
        }
        WsEvent::WindowClosed(window_id) => {
            let mut state = state?;
            remove_window_and_reselect_active(&mut state, window_id);
            Some(state)
        }
        WsEvent::WindowMoved { window_id, x, y } => {
            let mut state = state?;
            if let Some(window) = state.windows.iter_mut().find(|w| w.id == *window_id) {
                window.x = *x;
                window.y = *y;
            }
            Some(state)
        }
        WsEvent::WindowResized {
            window_id,
            width,
            height,
        } => {
            let mut state = state?;
            if let Some(window) = state.windows.iter_mut().find(|w| w.id == *window_id) {
                window.width = *width;
                window.height = *height;
            }
            Some(state)
        }
        WsEvent::WindowFocused { window_id, z_index } => {
            let mut state = state?;
            state.active_window = Some(window_id.clone());
            if let Some(window) = state.windows.iter_mut().find(|w| w.id == *window_id) {
                window.minimized = false;
                window.z_index = *z_index;
            }
            Some(state)
        }
        WsEvent::WindowMinimized(window_id) => {
            let mut state = state?;
            if let Some(window) = state.windows.iter_mut().find(|w| w.id == *window_id) {
                window.minimized = true;
                window.maximized = false;
            }

            if state.active_window.as_deref() == Some(window_id.as_str()) {
                state.active_window = state
                    .windows
                    .iter()
                    .filter(|w| !w.minimized)
                    .max_by_key(|w| w.z_index)
                    .map(|w| w.id.clone());
            }
            Some(state)
        }
        WsEvent::WindowMaximized {
            window_id,
            x,
            y,
            width,
            height,
        } => {
            let mut state = state?;
            let next_z = state.windows.iter().map(|w| w.z_index).max().unwrap_or(0) + 1;
            if let Some(window) = state.windows.iter_mut().find(|w| w.id == *window_id) {
                window.minimized = false;
                window.maximized = true;
                window.x = *x;
                window.y = *y;
                window.width = *width;
                window.height = *height;
                window.z_index = next_z;
            }
            state.active_window = Some(window_id.clone());
            Some(state)
        }
        WsEvent::WindowRestored {
            window_id,
            x,
            y,
            width,
            height,
            maximized,
        } => {
            let mut state = state?;
            let next_z = state.windows.iter().map(|w| w.z_index).max().unwrap_or(0) + 1;
            if let Some(window) = state.windows.iter_mut().find(|w| w.id == *window_id) {
                window.minimized = false;
                window.maximized = *maximized;
                window.x = *x;
                window.y = *y;
                window.width = *width;
                window.height = *height;
                window.z_index = next_z;
            }
            state.active_window = Some(window_id.clone());
            Some(state)
        }
        WsEvent::Connected
        | WsEvent::Disconnected
        | WsEvent::Pong
        | WsEvent::Error(_)
        | WsEvent::Telemetry { .. }
        | WsEvent::DocumentUpdate { .. }
        | WsEvent::WriterRunStarted { .. }
        | WsEvent::WriterRunProgress { .. }
        | WsEvent::WriterRunPatch { .. }
        | WsEvent::WriterRunStatus { .. }
        | WsEvent::WriterRunFailed { .. }
        | WsEvent::WriterRunChangeset { .. } => state,
    }
}

pub fn upsert_app(state: &mut DesktopState, app: AppDefinition) {
    if let Some(existing) = state.apps.iter_mut().find(|existing| existing.id == app.id) {
        *existing = app;
    } else {
        state.apps.push(app);
    }
}

pub fn push_window_and_activate(state: &mut DesktopState, window: WindowState) {
    let window_id = window.id.clone();
    state.windows.retain(|w| w.id != window_id);
    state.windows.push(window);
    state.active_window = Some(window_id);
}

pub fn remove_window_and_reselect_active(state: &mut DesktopState, window_id: &str) {
    state.windows.retain(|w| w.id != window_id);

    if state.active_window.as_deref() == Some(window_id) {
        state.active_window = state.windows.last().map(|w| w.id.clone());
    }
}

pub fn focus_window_and_raise_z(state: &mut DesktopState, window_id: &str) {
    state.active_window = Some(window_id.to_string());

    let max_z = state.windows.iter().map(|w| w.z_index).max().unwrap_or(0);
    if let Some(window) = state.windows.iter_mut().find(|w| w.id == window_id) {
        window.z_index = max_z + 1;
    }
}

#[cfg(test)]
mod ws_state_tests {
    use super::apply_writer_runs_event;
    use crate::desktop::ws::WsEvent;
    use chrono::{TimeZone, Utc};
    use shared_types::{ChangesetImpact, WriterRunEventBase};
    use std::collections::HashMap;

    #[test]
    fn changeset_events_attach_to_existing_writer_run_by_document_path() {
        let mut runs = HashMap::new();

        let base = WriterRunEventBase {
            desktop_id: "desktop-1".to_string(),
            session_id: "session-1".to_string(),
            thread_id: "thread-1".to_string(),
            run_id: "run-1".to_string(),
            document_path: "conductor/runs/run-1/draft.md".to_string(),
            revision: 4,
            head_version_id: None,
            timestamp: Utc.with_ymd_and_hms(2026, 3, 13, 22, 0, 0).unwrap(),
        };

        apply_writer_runs_event(
            &mut runs,
            &WsEvent::WriterRunStarted {
                base: base.clone(),
                objective: "Draft the answer".to_string(),
            },
        );
        apply_writer_runs_event(
            &mut runs,
            &WsEvent::WriterRunChangeset {
                base: base.clone(),
                patch_id: "patch-1".to_string(),
                loop_id: Some("loop-1".to_string()),
                summary: "Tightened the opening section.".to_string(),
                impact: ChangesetImpact::Medium,
                op_taxonomy: vec!["replace".to_string(), "clarification".to_string()],
            },
        );

        let run = runs
            .get("conductor/runs/run-1/draft.md")
            .expect("writer run should exist");
        assert_eq!(run.recent_changesets.len(), 1);
        assert_eq!(run.recent_changesets[0].patch_id, "patch-1");
        assert_eq!(
            run.recent_changesets[0].summary,
            "Tightened the opening section."
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_desktop_state_event, upsert_app};
    use crate::desktop::ws::WsEvent;
    use shared_types::{AppDefinition, DesktopState, WindowState};

    fn app(id: &str, name: &str) -> AppDefinition {
        AppDefinition {
            id: id.to_string(),
            name: name.to_string(),
            icon: "x".to_string(),
            component_code: format!("{name}View"),
            default_width: 640,
            default_height: 480,
        }
    }

    fn window(id: &str, z_index: u32) -> WindowState {
        WindowState {
            id: id.to_string(),
            app_id: "writer".to_string(),
            title: id.to_string(),
            x: 10,
            y: 10,
            width: 400,
            height: 300,
            z_index,
            minimized: false,
            maximized: false,
            props: serde_json::Value::Null,
        }
    }

    #[test]
    fn upsert_app_replaces_matching_id() {
        let mut state = DesktopState {
            windows: Vec::new(),
            active_window: None,
            apps: vec![app("writer", "Writer")],
        };

        upsert_app(&mut state, app("writer", "Writer 2"));

        assert_eq!(state.apps.len(), 1);
        assert_eq!(state.apps[0].name, "Writer 2");
    }

    #[test]
    fn apply_ws_event_app_registered_updates_app_catalog() {
        let state = apply_desktop_state_event(
            Some(DesktopState {
                windows: Vec::new(),
                active_window: None,
                apps: vec![app("writer", "Writer")],
            }),
            &WsEvent::AppRegistered(app("trace", "Trace")),
        );

        let state = state.expect("desktop state");
        assert_eq!(state.apps.len(), 2);
        assert!(state.apps.iter().any(|app| app.id == "trace"));
    }

    #[test]
    fn apply_ws_event_window_closed_reselects_active_window() {
        let state = apply_desktop_state_event(
            Some(DesktopState {
                windows: vec![window("left", 1), window("right", 2)],
                active_window: Some("right".to_string()),
                apps: Vec::new(),
            }),
            &WsEvent::WindowClosed("right".to_string()),
        );

        let state = state.expect("desktop state");
        assert_eq!(state.windows.len(), 1);
        assert_eq!(state.active_window.as_deref(), Some("left"));
    }

    #[test]
    fn apply_ws_event_window_restored_raises_window() {
        let state = apply_desktop_state_event(
            Some(DesktopState {
                windows: vec![
                    window("other", 3),
                    WindowState {
                        minimized: true,
                        ..window("restored", 1)
                    },
                ],
                active_window: Some("other".to_string()),
                apps: Vec::new(),
            }),
            &WsEvent::WindowRestored {
                window_id: "restored".to_string(),
                x: 20,
                y: 30,
                width: 500,
                height: 320,
                maximized: false,
            },
        );

        let state = state.expect("desktop state");
        let restored = state
            .windows
            .iter()
            .find(|window| window.id == "restored")
            .expect("restored window");
        assert!(!restored.minimized);
        assert_eq!(restored.z_index, 4);
        assert_eq!(state.active_window.as_deref(), Some("restored"));
    }
}
