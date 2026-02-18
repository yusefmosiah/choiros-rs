use dioxus::prelude::{Signal, WritableExt};
use shared_types::{
    ChangesetImpact, DesktopState, PatchOp, PatchSource, WindowState, WriterRunStatusKind,
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

/// Update the global writer runs state from a WsEvent
pub fn update_writer_runs_from_event(event: &WsEvent) {
    match event {
        WsEvent::WriterRunPatch { base, payload } => {
            let mut runs = ACTIVE_WRITER_RUNS.write();
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
                proposal: None,
                pending_patches: Vec::new(),
                last_applied_revision: 0,
                recent_changesets: Vec::new(),
            };
            ACTIVE_WRITER_RUNS
                .write()
                .insert(base.document_path.clone(), run);
        }
        WsEvent::WriterRunProgress {
            base,
            phase,
            message,
            progress_pct,
        } => {
            let mut runs = ACTIVE_WRITER_RUNS.write();
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
            let mut runs = ACTIVE_WRITER_RUNS.write();
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
            let mut runs = ACTIVE_WRITER_RUNS.write();
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
            let mut runs = ACTIVE_WRITER_RUNS.write();
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
        WsEvent::DesktopStateUpdate(state) => {
            desktop_state.set(Some(state));
        }
        WsEvent::WindowOpened(window) => {
            if let Some(state) = desktop_state.write().as_mut() {
                state.windows.retain(|w| w.id != window.id);
                state.windows.push(window);
            }
        }
        WsEvent::WindowClosed(window_id) => {
            if let Some(state) = desktop_state.write().as_mut() {
                state.windows.retain(|w| w.id != window_id);
            }
        }
        WsEvent::WindowMoved { window_id, x, y } => {
            if let Some(state) = desktop_state.write().as_mut() {
                if let Some(window) = state.windows.iter_mut().find(|w| w.id == window_id) {
                    window.x = x;
                    window.y = y;
                }
            }
        }
        WsEvent::WindowResized {
            window_id,
            width,
            height,
        } => {
            if let Some(state) = desktop_state.write().as_mut() {
                if let Some(window) = state.windows.iter_mut().find(|w| w.id == window_id) {
                    window.width = width;
                    window.height = height;
                }
            }
        }
        WsEvent::WindowFocused(window_id) => {
            if let Some(state) = desktop_state.write().as_mut() {
                state.active_window = Some(window_id.clone());
                if let Some(window) = state.windows.iter_mut().find(|w| w.id == window_id) {
                    window.minimized = false;
                }
            }
        }
        WsEvent::WindowMinimized(window_id) => {
            if let Some(state) = desktop_state.write().as_mut() {
                if let Some(window) = state.windows.iter_mut().find(|w| w.id == window_id) {
                    window.minimized = true;
                    window.maximized = false;
                }

                if state.active_window.as_deref() == Some(&window_id) {
                    state.active_window = state
                        .windows
                        .iter()
                        .filter(|w| !w.minimized)
                        .max_by_key(|w| w.z_index)
                        .map(|w| w.id.clone());
                }
            }
        }
        WsEvent::WindowMaximized {
            window_id,
            x,
            y,
            width,
            height,
        } => {
            if let Some(state) = desktop_state.write().as_mut() {
                let next_z = state.windows.iter().map(|w| w.z_index).max().unwrap_or(0) + 1;
                if let Some(window) = state.windows.iter_mut().find(|w| w.id == window_id) {
                    window.minimized = false;
                    window.maximized = true;
                    window.x = x;
                    window.y = y;
                    window.width = width;
                    window.height = height;
                    window.z_index = next_z;
                }
                state.active_window = Some(window_id);
            }
        }
        WsEvent::WindowRestored {
            window_id,
            x,
            y,
            width,
            height,
            maximized,
        } => {
            if let Some(state) = desktop_state.write().as_mut() {
                if let Some(window) = state.windows.iter_mut().find(|w| w.id == window_id) {
                    window.minimized = false;
                    window.maximized = maximized;
                    window.x = x;
                    window.y = y;
                    window.width = width;
                    window.height = height;
                }
                state.active_window = Some(window_id);
            }
        }
        WsEvent::Pong => {}
        WsEvent::Error(_) => {}
        WsEvent::Telemetry { .. } => {
            // Telemetry events are handled separately by the prompt bar
            // They don't modify desktop state
        }
        WsEvent::DocumentUpdate { .. } => {
            // Document update events are handled separately by the run view
            // They don't modify desktop state
        }
        WsEvent::WriterRunStarted { .. }
        | WsEvent::WriterRunProgress { .. }
        | WsEvent::WriterRunPatch { .. }
        | WsEvent::WriterRunStatus { .. }
        | WsEvent::WriterRunFailed { .. }
        | WsEvent::WriterRunChangeset { .. } => {
            // Writer run events are handled by the writer component
            // They don't modify desktop state directly
        }
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
