//! DesktopActor - manages window state and app registry
//!
//! PREDICTION: Window state can be managed as an actor projection from events,
//! enabling mobile-first responsive windows that persist across sessions.
//!
//! EXPERIMENT:
//! 1. DesktopActor owns all window state in SQLite
//! 2. UI renders projections, never owns state
//! 3. Supports mobile (single window) and desktop (multi-window) modes
//! 4. Dynamic app registration at runtime
//!
//! OBSERVE:
//! - Window state survives page refresh
//! - Same actor instance for same desktop_id
//! - Mobile-first: single window view, desktop: floating windows

use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use std::collections::HashMap;

use crate::actors::event_store::{AppendEvent, EventStoreError, EventStoreMsg};

/// Actor that manages desktop window state
#[derive(Debug, Default)]
pub struct DesktopActor;

/// Arguments for spawning DesktopActor
#[derive(Debug, Clone)]
pub struct DesktopArguments {
    pub desktop_id: String,
    pub user_id: String,
    pub event_store: ActorRef<EventStoreMsg>,
}

/// State for DesktopActor
pub struct DesktopState {
    desktop_id: String,
    user_id: String,
    windows: HashMap<String, shared_types::WindowState>,
    apps: HashMap<String, shared_types::AppDefinition>,
    active_window: Option<String>,
    next_z_index: u32,
    last_seq: i64,
    event_store: ActorRef<EventStoreMsg>,
}

// ============================================================================
// Messages
// ============================================================================

/// Messages handled by DesktopActor
#[derive(Debug)]
pub enum DesktopActorMsg {
    /// Open a new window for an app
    OpenWindow {
        app_id: String,
        title: String,
        props: Option<serde_json::Value>,
        reply: RpcReplyPort<Result<shared_types::WindowState, DesktopError>>,
    },
    /// Close a window
    CloseWindow {
        window_id: String,
        reply: RpcReplyPort<Result<(), DesktopError>>,
    },
    /// Move a window
    MoveWindow {
        window_id: String,
        x: i32,
        y: i32,
        reply: RpcReplyPort<Result<(), DesktopError>>,
    },
    /// Resize a window
    ResizeWindow {
        window_id: String,
        width: i32,
        height: i32,
        reply: RpcReplyPort<Result<(), DesktopError>>,
    },
    /// Focus a window (bring to front)
    FocusWindow {
        window_id: String,
        reply: RpcReplyPort<Result<(), DesktopError>>,
    },
    /// Get all windows
    GetWindows {
        reply: RpcReplyPort<Vec<shared_types::WindowState>>,
    },
    /// Get current desktop state
    GetDesktopState {
        reply: RpcReplyPort<shared_types::DesktopState>,
    },
    /// Register a new app
    RegisterApp {
        app: shared_types::AppDefinition,
        reply: RpcReplyPort<Result<(), DesktopError>>,
    },
    /// Get all registered apps
    GetApps {
        reply: RpcReplyPort<Vec<shared_types::AppDefinition>>,
    },
    /// Sync events (from EventStore)
    SyncEvents {
        events: Vec<shared_types::Event>,
    },
    /// Get actor info
    GetActorInfo {
        reply: RpcReplyPort<(String, String)>,
    },
}

// ============================================================================
// Event Types
// ============================================================================

const EVENT_WINDOW_OPENED: &str = "desktop.window_opened";
const EVENT_WINDOW_CLOSED: &str = "desktop.window_closed";
const EVENT_WINDOW_MOVED: &str = "desktop.window_moved";
const EVENT_WINDOW_RESIZED: &str = "desktop.window_resized";
const EVENT_WINDOW_FOCUSED: &str = "desktop.window_focused";
#[allow(dead_code)]
const EVENT_WINDOW_MINIMIZED: &str = "desktop.window_minimized";
#[allow(dead_code)]
const EVENT_WINDOW_MAXIMIZED: &str = "desktop.window_maximized";
const EVENT_APP_REGISTERED: &str = "desktop.app_registered";

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, thiserror::Error, Clone)]
pub enum DesktopError {
    #[error("Event store error: {0}")]
    EventStore(String),

    #[error("Window not found: {0}")]
    WindowNotFound(String),

    #[error("App not found: {0}")]
    AppNotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[allow(dead_code)]
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
}

impl From<serde_json::Error> for DesktopError {
    fn from(e: serde_json::Error) -> Self {
        DesktopError::Serialization(e.to_string())
    }
}

impl From<EventStoreError> for DesktopError {
    fn from(e: EventStoreError) -> Self {
        DesktopError::EventStore(e.to_string())
    }
}

// ============================================================================
// Actor Implementation
// ============================================================================

#[async_trait]
impl Actor for DesktopActor {
    type Msg = DesktopActorMsg;
    type State = DesktopState;
    type Arguments = DesktopArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        tracing::info!(
            actor_id = %myself.get_id(),
            desktop_id = %args.desktop_id,
            "DesktopActor starting"
        );

        let mut state = DesktopState {
            desktop_id: args.desktop_id,
            user_id: args.user_id,
            windows: HashMap::new(),
            apps: HashMap::new(),
            active_window: None,
            next_z_index: 100,
            last_seq: 0,
            event_store: args.event_store,
        };

        // Register default apps if none exist
        if state.apps.is_empty() {
            state.apps.insert(
                "chat".to_string(),
                shared_types::AppDefinition {
                    id: "chat".to_string(),
                    name: "Chat".to_string(),
                    icon: "💬".to_string(),
                    component_code: "ChatApp".to_string(),
                    default_width: 800,
                    default_height: 600,
                },
            );
        }

        Ok(state)
    }

    async fn post_start(
        &self,
        myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!(
            actor_id = %myself.get_id(),
            "DesktopActor started successfully"
        );
        Ok(())
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            DesktopActorMsg::OpenWindow {
                app_id,
                title,
                props,
                reply,
            } => {
                let result = self.handle_open_window(app_id, title, props, state).await;
                let _ = reply.send(result);
            }
            DesktopActorMsg::CloseWindow { window_id, reply } => {
                let result = self.handle_close_window(window_id, state).await;
                let _ = reply.send(result);
            }
            DesktopActorMsg::MoveWindow {
                window_id,
                x,
                y,
                reply,
            } => {
                let result = self.handle_move_window(window_id, x, y, state).await;
                let _ = reply.send(result);
            }
            DesktopActorMsg::ResizeWindow {
                window_id,
                width,
                height,
                reply,
            } => {
                let result = self.handle_resize_window(window_id, width, height, state).await;
                let _ = reply.send(result);
            }
            DesktopActorMsg::FocusWindow { window_id, reply } => {
                let result = self.handle_focus_window(window_id, state).await;
                let _ = reply.send(result);
            }
            DesktopActorMsg::GetWindows { reply } => {
                let result = self.handle_get_windows(state);
                let _ = reply.send(result);
            }
            DesktopActorMsg::GetDesktopState { reply } => {
                let result = self.handle_get_desktop_state(state).await;
                let _ = reply.send(result);
            }
            DesktopActorMsg::RegisterApp { app, reply } => {
                let result = self.handle_register_app(app, state).await;
                let _ = reply.send(result);
            }
            DesktopActorMsg::GetApps { reply } => {
                let result = self.handle_get_apps(state);
                let _ = reply.send(result);
            }
            DesktopActorMsg::SyncEvents { events } => {
                self.project_events(events, state);
            }
            DesktopActorMsg::GetActorInfo { reply } => {
                let result = (state.desktop_id.clone(), state.user_id.clone());
                let _ = reply.send(result);
            }
        }
        Ok(())
    }

    async fn post_stop(
        &self,
        myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!(
            actor_id = %myself.get_id(),
            "DesktopActor stopped"
        );
        Ok(())
    }
}

// ============================================================================
// Message Handlers
// ============================================================================

impl DesktopActor {
    /// Get next z-index and increment counter
    fn next_z(&self, state: &mut DesktopState) -> u32 {
        let z = state.next_z_index;
        state.next_z_index += 1;
        z
    }

    /// Calculate default window position (cascade from existing windows)
    fn get_default_position(&self, state: &DesktopState, _app_id: &str) -> (i32, i32) {
        let count = state.windows.len() as i32;
        let offset = count * 30;
        (100 + offset, 100 + offset)
    }

    /// Project events to update window/app state
    fn project_events(&self, events: Vec<shared_types::Event>, state: &mut DesktopState) {
        for event in events {
            state.last_seq = event.seq;

            match event.event_type.as_str() {
                EVENT_WINDOW_OPENED => {
                    if let Ok(window) =
                        serde_json::from_value::<shared_types::WindowState>(event.payload.clone())
                    {
                        state.windows.insert(window.id.clone(), window);
                    }
                }
                EVENT_WINDOW_CLOSED => {
                    if let Ok(payload) =
                        serde_json::from_value::<serde_json::Value>(event.payload.clone())
                    {
                        if let Some(window_id) = payload.get("window_id").and_then(|v| v.as_str()) {
                            state.windows.remove(window_id);
                            if state.active_window.as_deref() == Some(window_id) {
                                state.active_window = None;
                            }
                        }
                    }
                }
                EVENT_WINDOW_MOVED => {
                    if let Ok(payload) =
                        serde_json::from_value::<serde_json::Value>(event.payload.clone())
                    {
                        if let Some(window_id) = payload.get("window_id").and_then(|v| v.as_str()) {
                            if let Some(window) = state.windows.get_mut(window_id) {
                                if let Some(x) = payload.get("x").and_then(|v| v.as_i64()) {
                                    window.x = x as i32;
                                }
                                if let Some(y) = payload.get("y").and_then(|v| v.as_i64()) {
                                    window.y = y as i32;
                                }
                            }
                        }
                    }
                }
                EVENT_WINDOW_RESIZED => {
                    if let Ok(payload) =
                        serde_json::from_value::<serde_json::Value>(event.payload.clone())
                    {
                        if let Some(window_id) = payload.get("window_id").and_then(|v| v.as_str()) {
                            if let Some(window) = state.windows.get_mut(window_id) {
                                if let Some(width) = payload.get("width").and_then(|v| v.as_i64()) {
                                    window.width = width as i32;
                                }
                                if let Some(height) = payload.get("height").and_then(|v| v.as_i64())
                                {
                                    window.height = height as i32;
                                }
                            }
                        }
                    }
                }
                EVENT_WINDOW_FOCUSED => {
                    if let Ok(payload) =
                        serde_json::from_value::<serde_json::Value>(event.payload.clone())
                    {
                        if let Some(window_id) = payload.get("window_id").and_then(|v| v.as_str()) {
                            state.active_window = Some(window_id.to_string());
                            let new_z = self.next_z(state);
                            if let Some(window) = state.windows.get_mut(window_id) {
                                window.z_index = new_z;
                            }
                        }
                    }
                }
                EVENT_APP_REGISTERED => {
                    if let Ok(app) =
                        serde_json::from_value::<shared_types::AppDefinition>(event.payload.clone())
                    {
                        state.apps.insert(app.id.clone(), app);
                    }
                }
                _ => {} // Ignore other event types
            }
        }
    }

    /// Sync with EventStore - load historical events
    async fn sync_with_event_store(&self, state: &mut DesktopState) -> Option<Vec<shared_types::Event>> {
        let result: Result<Result<Vec<shared_types::Event>, EventStoreError>, ractor::RactorErr<EventStoreMsg>> =
            ractor::call!(
                &state.event_store,
                |reply| EventStoreMsg::GetEventsForActor {
                    actor_id: state.desktop_id.clone(),
                    since_seq: state.last_seq,
                    reply,
                }
            );

        match result {
            Ok(Ok(events)) => Some(events),
            _ => None,
        }
    }

    /// Append event to EventStore and return unit result
    async fn append_event_unit(
        &self,
        event_type: &str,
        payload: serde_json::Value,
        state: &DesktopState,
    ) -> Result<(), DesktopError> {
        let result: Result<Result<shared_types::Event, EventStoreError>, ractor::RactorErr<EventStoreMsg>> =
            ractor::call!(
                &state.event_store,
                |reply| EventStoreMsg::Append {
                    event: AppendEvent {
                        event_type: event_type.to_string(),
                        payload,
                        actor_id: state.desktop_id.clone(),
                        user_id: state.user_id.clone(),
                    },
                    reply,
                }
            );

        match result {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(DesktopError::EventStore(e.to_string())),
            Err(e) => Err(DesktopError::EventStore(format!("RPC error: {}", e))),
        }
    }

    /// Append event to EventStore and return the event
    #[allow(dead_code)]
    async fn append_event(
        &self,
        event_type: &str,
        payload: serde_json::Value,
        state: &DesktopState,
    ) -> Result<shared_types::Event, DesktopError> {
        let result: Result<Result<shared_types::Event, EventStoreError>, ractor::RactorErr<EventStoreMsg>> =
            ractor::call!(
                &state.event_store,
                |reply| EventStoreMsg::Append {
                    event: AppendEvent {
                        event_type: event_type.to_string(),
                        payload,
                        actor_id: state.desktop_id.clone(),
                        user_id: state.user_id.clone(),
                    },
                    reply,
                }
            );

        match result {
            Ok(Ok(event)) => Ok(event),
            Ok(Err(e)) => Err(DesktopError::EventStore(e.to_string())),
            Err(e) => Err(DesktopError::EventStore(format!("RPC error: {}", e))),
        }
    }

    async fn handle_open_window(
        &self,
        app_id: String,
        title: String,
        props: Option<serde_json::Value>,
        state: &mut DesktopState,
    ) -> Result<shared_types::WindowState, DesktopError> {
        // Check if app exists
        let app = match state.apps.get(&app_id) {
            Some(app) => app.clone(),
            None => {
                return Err(DesktopError::AppNotFound(app_id));
            }
        };

        // Create window state
        let window_id = ulid::Ulid::new().to_string();
        let (x, y) = self.get_default_position(state, &app_id);

        let window = shared_types::WindowState {
            id: window_id.clone(),
            app_id: app_id.clone(),
            title: title.clone(),
            x,
            y,
            width: app.default_width,
            height: app.default_height,
            z_index: self.next_z(state),
            minimized: false,
            maximized: false,
            props: props.unwrap_or_else(|| serde_json::json!({})),
        };

        // Store in memory
        state.windows.insert(window_id.clone(), window.clone());
        state.active_window = Some(window_id.clone());

        // Append event to EventStore
        let payload = serde_json::to_value(&window)?;
        self.append_event_unit(EVENT_WINDOW_OPENED, payload, state).await?;

        Ok(window)
    }

    async fn handle_close_window(
        &self,
        window_id: String,
        state: &mut DesktopState,
    ) -> Result<(), DesktopError> {
        // Remove from memory
        if state.windows.remove(&window_id).is_none() {
            return Err(DesktopError::WindowNotFound(window_id));
        }

        // Update active window
        if state.active_window.as_deref() == Some(&window_id) {
            state.active_window = state.windows.keys().next().cloned();
        }

        // Append event
        let payload = serde_json::json!({"window_id": window_id});
        self.append_event_unit(EVENT_WINDOW_CLOSED, payload, state).await
    }

    async fn handle_move_window(
        &self,
        window_id: String,
        x: i32,
        y: i32,
        state: &mut DesktopState,
    ) -> Result<(), DesktopError> {
        // Update memory
        if let Some(window) = state.windows.get_mut(&window_id) {
            window.x = x;
            window.y = y;
        } else {
            return Err(DesktopError::WindowNotFound(window_id));
        }

        // Append event
        let payload = serde_json::json!({
            "window_id": window_id,
            "x": x,
            "y": y,
        });
        self.append_event_unit(EVENT_WINDOW_MOVED, payload, state).await
    }

    async fn handle_resize_window(
        &self,
        window_id: String,
        width: i32,
        height: i32,
        state: &mut DesktopState,
    ) -> Result<(), DesktopError> {
        // Update memory
        if let Some(window) = state.windows.get_mut(&window_id) {
            window.width = width;
            window.height = height;
        } else {
            return Err(DesktopError::WindowNotFound(window_id));
        }

        // Append event
        let payload = serde_json::json!({
            "window_id": window_id,
            "width": width,
            "height": height,
        });
        self.append_event_unit(EVENT_WINDOW_RESIZED, payload, state).await
    }

    async fn handle_focus_window(
        &self,
        window_id: String,
        state: &mut DesktopState,
    ) -> Result<(), DesktopError> {
        // Check window exists
        if !state.windows.contains_key(&window_id) {
            return Err(DesktopError::WindowNotFound(window_id));
        }

        // Update state
        state.active_window = Some(window_id.clone());
        // Get z-index first to avoid borrow issues
        let new_z = self.next_z(state);
        if let Some(window) = state.windows.get_mut(&window_id) {
            window.z_index = new_z;
        }

        // Append event
        let payload = serde_json::json!({"window_id": window_id});
        self.append_event_unit(EVENT_WINDOW_FOCUSED, payload, state).await
    }

    fn handle_get_windows(&self, state: &DesktopState) -> Vec<shared_types::WindowState> {
        let mut windows: Vec<_> = state.windows.values().cloned().collect();
        // Sort by z-index
        windows.sort_by_key(|w| w.z_index);
        windows
    }

    async fn handle_get_desktop_state(
        &self,
        state: &mut DesktopState,
    ) -> shared_types::DesktopState {
        // If events haven't been synced yet, sync first
        if state.last_seq == 0 {
            if let Some(events) = self.sync_with_event_store(state).await {
                self.project_events(events, state);
            }

            // Ensure default apps exist if none loaded
            if state.apps.is_empty() {
                state.apps.insert(
                    "chat".to_string(),
                    shared_types::AppDefinition {
                        id: "chat".to_string(),
                        name: "Chat".to_string(),
                        icon: "💬".to_string(),
                        component_code: "ChatApp".to_string(),
                        default_width: 800,
                        default_height: 600,
                    },
                );
            }
        }

        // Build and return state
        let windows: Vec<_> = state.windows.values().cloned().collect();
        let active_window = state.active_window.clone();
        let apps: Vec<_> = state.apps.values().cloned().collect();

        shared_types::DesktopState {
            windows,
            active_window,
            apps,
        }
    }

    async fn handle_register_app(
        &self,
        app: shared_types::AppDefinition,
        state: &mut DesktopState,
    ) -> Result<(), DesktopError> {
        // Store in memory
        state.apps.insert(app.id.clone(), app.clone());

        // Append event
        let payload = serde_json::to_value(&app)?;
        self.append_event_unit(EVENT_APP_REGISTERED, payload, state).await
    }

    fn handle_get_apps(&self, state: &DesktopState) -> Vec<shared_types::AppDefinition> {
        state.apps.values().cloned().collect()
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Convenience function to open a window
pub async fn open_window(
    desktop: &ActorRef<DesktopActorMsg>,
    app_id: impl Into<String>,
    title: impl Into<String>,
    props: Option<serde_json::Value>,
) -> Result<Result<shared_types::WindowState, DesktopError>, ractor::RactorErr<DesktopActorMsg>> {
    ractor::call!(desktop, |reply| DesktopActorMsg::OpenWindow {
        app_id: app_id.into(),
        title: title.into(),
        props,
        reply,
    })
}

/// Convenience function to close a window
pub async fn close_window(
    desktop: &ActorRef<DesktopActorMsg>,
    window_id: impl Into<String>,
) -> Result<Result<(), DesktopError>, ractor::RactorErr<DesktopActorMsg>> {
    ractor::call!(desktop, |reply| DesktopActorMsg::CloseWindow {
        window_id: window_id.into(),
        reply,
    })
}

/// Convenience function to move a window
pub async fn move_window(
    desktop: &ActorRef<DesktopActorMsg>,
    window_id: impl Into<String>,
    x: i32,
    y: i32,
) -> Result<Result<(), DesktopError>, ractor::RactorErr<DesktopActorMsg>> {
    ractor::call!(desktop, |reply| DesktopActorMsg::MoveWindow {
        window_id: window_id.into(),
        x,
        y,
        reply,
    })
}

/// Convenience function to resize a window
pub async fn resize_window(
    desktop: &ActorRef<DesktopActorMsg>,
    window_id: impl Into<String>,
    width: i32,
    height: i32,
) -> Result<Result<(), DesktopError>, ractor::RactorErr<DesktopActorMsg>> {
    ractor::call!(desktop, |reply| DesktopActorMsg::ResizeWindow {
        window_id: window_id.into(),
        width,
        height,
        reply,
    })
}

/// Convenience function to focus a window
pub async fn focus_window(
    desktop: &ActorRef<DesktopActorMsg>,
    window_id: impl Into<String>,
) -> Result<Result<(), DesktopError>, ractor::RactorErr<DesktopActorMsg>> {
    ractor::call!(desktop, |reply| DesktopActorMsg::FocusWindow {
        window_id: window_id.into(),
        reply,
    })
}

/// Convenience function to get all windows
pub async fn get_windows(
    desktop: &ActorRef<DesktopActorMsg>,
) -> Result<Vec<shared_types::WindowState>, ractor::RactorErr<DesktopActorMsg>> {
    ractor::call!(desktop, |reply| DesktopActorMsg::GetWindows { reply })
}

/// Convenience function to get desktop state
pub async fn get_desktop_state(
    desktop: &ActorRef<DesktopActorMsg>,
) -> Result<shared_types::DesktopState, ractor::RactorErr<DesktopActorMsg>> {
    ractor::call!(desktop, |reply| DesktopActorMsg::GetDesktopState { reply })
}

/// Convenience function to register an app
pub async fn register_app(
    desktop: &ActorRef<DesktopActorMsg>,
    app: shared_types::AppDefinition,
) -> Result<Result<(), DesktopError>, ractor::RactorErr<DesktopActorMsg>> {
    ractor::call!(desktop, |reply| DesktopActorMsg::RegisterApp { app, reply })
}

/// Convenience function to get all apps
pub async fn get_apps(
    desktop: &ActorRef<DesktopActorMsg>,
) -> Result<Vec<shared_types::AppDefinition>, ractor::RactorErr<DesktopActorMsg>> {
    ractor::call!(desktop, |reply| DesktopActorMsg::GetApps { reply })
}

/// Convenience function to sync events
pub async fn sync_events(
    desktop: &ActorRef<DesktopActorMsg>,
    events: Vec<shared_types::Event>,
) -> Result<(), ractor::RactorErr<DesktopActorMsg>> {
    desktop.cast(DesktopActorMsg::SyncEvents { events }).map_err(|e| ractor::RactorErr::from(e))
}

/// Convenience function to get actor info
pub async fn get_actor_info(
    desktop: &ActorRef<DesktopActorMsg>,
) -> Result<(String, String), ractor::RactorErr<DesktopActorMsg>> {
    ractor::call!(desktop, |reply| DesktopActorMsg::GetActorInfo { reply })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::event_store::{EventStoreActor, EventStoreArguments};
    use ractor::Actor;

    // ============================================================================
    // Test 1: Opening window creates it with proper defaults
    // ============================================================================

    #[tokio::test]
    async fn test_open_window_creates_window() {
        let (event_store, _handle) = Actor::spawn(
            None,
            EventStoreActor,
            EventStoreArguments::InMemory,
        )
        .await
        .unwrap();

        let (desktop, _handle) = Actor::spawn(
            None,
            DesktopActor,
            DesktopArguments {
                desktop_id: "desktop-1".to_string(),
                user_id: "user-1".to_string(),
                event_store: event_store.clone(),
            },
        )
        .await
        .unwrap();

        // Register an app first
        let _ = register_app(
            &desktop,
            shared_types::AppDefinition {
                id: "chat".to_string(),
                name: "Chat".to_string(),
                icon: "💬".to_string(),
                component_code: "ChatApp".to_string(),
                default_width: 800,
                default_height: 600,
            },
        )
        .await
        .unwrap();

        // Open a window
        let window = open_window(&desktop, "chat", "Chat Window", None)
            .await
            .unwrap();

        assert!(window.is_ok());
        let window = window.unwrap();
        assert_eq!(window.app_id, "chat");
        assert_eq!(window.title, "Chat Window");
        assert_eq!(window.width, 800);
        assert_eq!(window.height, 600);
        assert!(!window.minimized);

        // Cleanup
        desktop.stop(None);
        event_store.stop(None);
    }

    // ============================================================================
    // Test 2: Opening window for non-existent app fails
    // ============================================================================

    #[tokio::test]
    async fn test_open_window_unknown_app_fails() {
        let (event_store, _handle) = Actor::spawn(
            None,
            EventStoreActor,
            EventStoreArguments::InMemory,
        )
        .await
        .unwrap();

        let (desktop, _handle) = Actor::spawn(
            None,
            DesktopActor,
            DesktopArguments {
                desktop_id: "desktop-1".to_string(),
                user_id: "user-1".to_string(),
                event_store: event_store.clone(),
            },
        )
        .await
        .unwrap();

        // Try to open window for unknown app
        let result = open_window(&desktop, "unknown", "Unknown", None).await;

        assert!(result.is_ok()); // RPC OK
        let inner = result.unwrap();
        assert!(inner.is_err()); // Handler returned error

        // Cleanup
        desktop.stop(None);
        event_store.stop(None);
    }

    // ============================================================================
    // Test 3: Closing window removes it
    // ============================================================================

    #[tokio::test]
    async fn test_close_window_removes_it() {
        let (event_store, _handle) = Actor::spawn(
            None,
            EventStoreActor,
            EventStoreArguments::InMemory,
        )
        .await
        .unwrap();

        let (desktop, _handle) = Actor::spawn(
            None,
            DesktopActor,
            DesktopArguments {
                desktop_id: "desktop-1".to_string(),
                user_id: "user-1".to_string(),
                event_store: event_store.clone(),
            },
        )
        .await
        .unwrap();

        // Register app and open window
        let _ = register_app(
            &desktop,
            shared_types::AppDefinition {
                id: "chat".to_string(),
                name: "Chat".to_string(),
                icon: "💬".to_string(),
                component_code: "ChatApp".to_string(),
                default_width: 800,
                default_height: 600,
            },
        )
        .await
        .unwrap();

        let window = open_window(&desktop, "chat", "Chat", None)
            .await
            .unwrap()
            .unwrap();

        let window_id = window.id;

        // Close the window
        let result = close_window(&desktop, &window_id).await.unwrap();

        assert!(result.is_ok());

        // Verify window is gone
        let windows = get_windows(&desktop).await.unwrap();
        assert!(windows.is_empty());

        // Cleanup
        desktop.stop(None);
    }

    // ============================================================================
    // Test 4: Moving window updates position
    // ============================================================================

    #[tokio::test]
    async fn test_move_window_updates_position() {
        let (event_store, _handle) = Actor::spawn(
            None,
            EventStoreActor,
            EventStoreArguments::InMemory,
        )
        .await
        .unwrap();

        let (desktop, _handle) = Actor::spawn(
            None,
            DesktopActor,
            DesktopArguments {
                desktop_id: "desktop-1".to_string(),
                user_id: "user-1".to_string(),
                event_store: event_store.clone(),
            },
        )
        .await
        .unwrap();

        // Register app and open window
        let _ = register_app(
            &desktop,
            shared_types::AppDefinition {
                id: "chat".to_string(),
                name: "Chat".to_string(),
                icon: "💬".to_string(),
                component_code: "ChatApp".to_string(),
                default_width: 800,
                default_height: 600,
            },
        )
        .await
        .unwrap();

        let window = open_window(&desktop, "chat", "Chat", None)
            .await
            .unwrap()
            .unwrap();

        let window_id = window.id;

        // Move the window
        let result = move_window(&desktop, &window_id, 200, 300).await.unwrap();

        assert!(result.is_ok());

        // Verify position updated
        let windows = get_windows(&desktop).await.unwrap();
        assert_eq!(windows[0].x, 200);
        assert_eq!(windows[0].y, 300);

        // Cleanup
        desktop.stop(None);
    }

    // ============================================================================
    // Test 5: Focus window brings it to front (highest z-index)
    // ============================================================================

    #[tokio::test]
    async fn test_focus_window_brings_to_front() {
        let (event_store, _handle) = Actor::spawn(
            None,
            EventStoreActor,
            EventStoreArguments::InMemory,
        )
        .await
        .unwrap();

        let (desktop, _handle) = Actor::spawn(
            None,
            DesktopActor,
            DesktopArguments {
                desktop_id: "desktop-1".to_string(),
                user_id: "user-1".to_string(),
                event_store: event_store.clone(),
            },
        )
        .await
        .unwrap();

        // Register app and open two windows
        let _ = register_app(
            &desktop,
            shared_types::AppDefinition {
                id: "chat".to_string(),
                name: "Chat".to_string(),
                icon: "💬".to_string(),
                component_code: "ChatApp".to_string(),
                default_width: 800,
                default_height: 600,
            },
        )
        .await
        .unwrap();

        let window1 = open_window(&desktop, "chat", "Window 1", None)
            .await
            .unwrap()
            .unwrap();

        let window2 = open_window(&desktop, "chat", "Window 2", None)
            .await
            .unwrap()
            .unwrap();

        // Window 2 should have higher z-index
        assert!(window2.z_index > window1.z_index);

        // Focus window 1
        let _ = focus_window(&desktop, &window1.id).await.unwrap();

        // Window 1 should now have higher z-index than window 2 had
        let windows = get_windows(&desktop).await.unwrap();
        let w1 = windows.iter().find(|w| w.id == window1.id).unwrap();
        assert!(w1.z_index > window2.z_index);

        // Cleanup
        desktop.stop(None);
    }

    // ============================================================================
    // Test 6: Get desktop state returns all windows and apps
    // ============================================================================

    #[tokio::test]
    async fn test_get_desktop_state() {
        let (event_store, _handle) = Actor::spawn(
            None,
            EventStoreActor,
            EventStoreArguments::InMemory,
        )
        .await
        .unwrap();

        let (desktop, _handle) = Actor::spawn(
            None,
            DesktopActor,
            DesktopArguments {
                desktop_id: "desktop-1".to_string(),
                user_id: "user-1".to_string(),
                event_store: event_store.clone(),
            },
        )
        .await
        .unwrap();

        // Register app
        let _ = register_app(
            &desktop,
            shared_types::AppDefinition {
                id: "chat".to_string(),
                name: "Chat".to_string(),
                icon: "💬".to_string(),
                component_code: "ChatApp".to_string(),
                default_width: 800,
                default_height: 600,
            },
        )
        .await
        .unwrap();

        // Open window
        let window = open_window(&desktop, "chat", "Chat", None)
            .await
            .unwrap()
            .unwrap();

        // Get desktop state
        let state = get_desktop_state(&desktop).await.unwrap();

        assert_eq!(state.windows.len(), 1);
        assert_eq!(state.apps.len(), 1); // Only the registered chat app (same id replaces default)
        assert_eq!(state.active_window, Some(window.id));

        // Cleanup
        desktop.stop(None);
    }

    // ============================================================================
    // Test 7: Registering app adds it to registry
    // ============================================================================

    #[tokio::test]
    async fn test_register_app() {
        let (event_store, _handle) = Actor::spawn(
            None,
            EventStoreActor,
            EventStoreArguments::InMemory,
        )
        .await
        .unwrap();

        let (desktop, _handle) = Actor::spawn(
            None,
            DesktopActor,
            DesktopArguments {
                desktop_id: "desktop-1".to_string(),
                user_id: "user-1".to_string(),
                event_store: event_store.clone(),
            },
        )
        .await
        .unwrap();

        // Register a new app (chat app is added by default on startup)
        let result = register_app(
            &desktop,
            shared_types::AppDefinition {
                id: "calc".to_string(),
                name: "Calculator".to_string(),
                icon: "🧮".to_string(),
                component_code: "CalcApp".to_string(),
                default_width: 300,
                default_height: 400,
            },
        )
        .await
        .unwrap();

        assert!(result.is_ok());

        let apps = get_apps(&desktop).await.unwrap();
        // Should have 2 apps: default chat + registered calc
        assert_eq!(apps.len(), 2);
        assert!(apps.iter().any(|a| a.id == "chat"));
        assert!(apps.iter().any(|a| a.id == "calc"));

        // Cleanup
        desktop.stop(None);
    }
}
