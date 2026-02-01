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

use actix::{
    Actor, ActorFutureExt, Addr, AsyncContext, Context, Handler, Message, Supervised, WrapFuture,
};
use std::collections::HashMap;

use crate::actors::event_store::{AppendEvent, EventStoreActor, GetEventsForActor};

/// Actor that manages desktop window state
pub struct DesktopActor {
    desktop_id: String,
    user_id: String,
    windows: HashMap<String, shared_types::WindowState>,
    apps: HashMap<String, shared_types::AppDefinition>,
    active_window: Option<String>,
    next_z_index: u32,
    last_seq: i64,
    event_store: Option<Addr<EventStoreActor>>,
}

impl DesktopActor {
    pub fn new(desktop_id: String, user_id: String, event_store: Addr<EventStoreActor>) -> Self {
        Self {
            desktop_id,
            user_id,
            windows: HashMap::new(),
            apps: HashMap::new(),
            active_window: None,
            next_z_index: 100,
            last_seq: 0,
            event_store: Some(event_store),
        }
    }

    /// Get next z-index and increment counter
    fn next_z(&mut self) -> u32 {
        let z = self.next_z_index;
        self.next_z_index += 1;
        z
    }

    /// Calculate default window position (cascade from existing windows)
    fn get_default_position(&self, _app_id: &str) -> (i32, i32) {
        let count = self.windows.len() as i32;
        let offset = count * 30;
        (100 + offset, 100 + offset)
    }

    /// Project events to update window/app state
    fn project_events(&mut self, events: Vec<shared_types::Event>) {
        for event in events {
            self.last_seq = event.seq;

            match event.event_type.as_str() {
                EVENT_WINDOW_OPENED => {
                    if let Ok(window) =
                        serde_json::from_value::<shared_types::WindowState>(event.payload.clone())
                    {
                        self.windows.insert(window.id.clone(), window);
                    }
                }
                EVENT_WINDOW_CLOSED => {
                    if let Ok(payload) =
                        serde_json::from_value::<serde_json::Value>(event.payload.clone())
                    {
                        if let Some(window_id) = payload.get("window_id").and_then(|v| v.as_str()) {
                            self.windows.remove(window_id);
                            if self.active_window.as_deref() == Some(window_id) {
                                self.active_window = None;
                            }
                        }
                    }
                }
                EVENT_WINDOW_MOVED => {
                    if let Ok(payload) =
                        serde_json::from_value::<serde_json::Value>(event.payload.clone())
                    {
                        if let Some(window_id) = payload.get("window_id").and_then(|v| v.as_str()) {
                            if let Some(window) = self.windows.get_mut(window_id) {
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
                            if let Some(window) = self.windows.get_mut(window_id) {
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
                            self.active_window = Some(window_id.to_string());
                            let new_z = self.next_z();
                            if let Some(window) = self.windows.get_mut(window_id) {
                                window.z_index = new_z;
                            }
                        }
                    }
                }
                EVENT_APP_REGISTERED => {
                    if let Ok(app) =
                        serde_json::from_value::<shared_types::AppDefinition>(event.payload.clone())
                    {
                        self.apps.insert(app.id.clone(), app);
                    }
                }
                _ => {} // Ignore other event types
            }
        }
    }

    /// Sync with EventStore - load historical events
    fn sync_with_event_store(&mut self, ctx: &mut Context<Self>) {
        if let Some(event_store) = self.event_store.clone() {
            let desktop_id = self.desktop_id.clone();
            let last_seq = self.last_seq;

            let fut = async move {
                let result: Result<
                    Result<Vec<shared_types::Event>, crate::actors::event_store::EventStoreError>,
                    actix::MailboxError,
                > = event_store
                    .send(GetEventsForActor {
                        actor_id: desktop_id,
                        since_seq: last_seq,
                    })
                    .await;

                match result {
                    Ok(Ok(events)) => Some(events),
                    _ => None,
                }
            };

            ctx.spawn(fut.into_actor(self).map(
                |events: Option<Vec<shared_types::Event>>, actor: &mut DesktopActor, _| {
                    if let Some(events) = events {
                        actor.project_events(events);
                    }
                },
            ));
        }
    }

    /// Append event to EventStore and return unit result
    fn append_event_unit(
        &self,
        event_type: &str,
        payload: serde_json::Value,
    ) -> actix::ResponseActFuture<Self, Result<(), DesktopError>> {
        let event_store = self.event_store.clone();
        let desktop_id = self.desktop_id.clone();
        let user_id = self.user_id.clone();
        let event_type = event_type.to_string();

        Box::pin(
            async move {
                if let Some(es) = event_store {
                    match es
                        .send(AppendEvent {
                            event_type: event_type.clone(),
                            payload,
                            actor_id: desktop_id,
                            user_id,
                        })
                        .await
                    {
                        Ok(Ok(_)) => Ok(()),
                        Ok(Err(e)) => Err(DesktopError::EventStore(e.to_string())),
                        Err(_) => Err(DesktopError::EventStore("Mailbox error".to_string())),
                    }
                } else {
                    Err(DesktopError::EventStore("No event store".to_string()))
                }
            }
            .into_actor(self),
        )
    }

    /// Append event to EventStore and return the event
    fn append_event(
        &self,
        event_type: &str,
        payload: serde_json::Value,
    ) -> actix::ResponseActFuture<Self, Result<shared_types::Event, DesktopError>> {
        let event_store = self.event_store.clone();
        let desktop_id = self.desktop_id.clone();
        let user_id = self.user_id.clone();
        let event_type = event_type.to_string();

        Box::pin(
            async move {
                if let Some(es) = event_store {
                    match es
                        .send(AppendEvent {
                            event_type: event_type.clone(),
                            payload,
                            actor_id: desktop_id,
                            user_id,
                        })
                        .await
                    {
                        Ok(Ok(event)) => Ok(event),
                        Ok(Err(e)) => Err(DesktopError::EventStore(e.to_string())),
                        Err(_) => Err(DesktopError::EventStore("Mailbox error".to_string())),
                    }
                } else {
                    Err(DesktopError::EventStore("No event store".to_string()))
                }
            }
            .into_actor(self),
        )
    }
}

impl Actor for DesktopActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        // Register default apps if none exist
        // Note: Event sync is now done lazily in GetDesktopState handler
        if self.apps.is_empty() {
            self.apps.insert(
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
}

// Implement Supervised for fault tolerance
impl Supervised for DesktopActor {
    fn restarting(&mut self, ctx: &mut Context<Self>) {
        // Clear in-memory state but keep identity
        self.windows.clear();
        self.apps.clear();
        self.active_window = None;
        self.next_z_index = 100;
        self.last_seq = 0;

        // Re-sync with EventStore
        self.sync_with_event_store(ctx);
    }
}

// ============================================================================
// Event Types
// ============================================================================

const EVENT_WINDOW_OPENED: &str = "desktop.window_opened";
const EVENT_WINDOW_CLOSED: &str = "desktop.window_closed";
const EVENT_WINDOW_MOVED: &str = "desktop.window_moved";
const EVENT_WINDOW_RESIZED: &str = "desktop.window_resized";
const EVENT_WINDOW_FOCUSED: &str = "desktop.window_focused";
const EVENT_WINDOW_MINIMIZED: &str = "desktop.window_minimized";
const EVENT_WINDOW_MAXIMIZED: &str = "desktop.window_maximized";
const EVENT_APP_REGISTERED: &str = "desktop.app_registered";

// ============================================================================
// Messages
// ============================================================================

/// Open a new window for an app
#[derive(Message)]
#[rtype(result = "Result<shared_types::WindowState, DesktopError>")]
pub struct OpenWindow {
    pub app_id: String,
    pub title: String,
    pub props: Option<serde_json::Value>,
}

/// Close a window
#[derive(Message)]
#[rtype(result = "Result<(), DesktopError>")]
pub struct CloseWindow {
    pub window_id: String,
}

/// Move a window
#[derive(Message)]
#[rtype(result = "Result<(), DesktopError>")]
pub struct MoveWindow {
    pub window_id: String,
    pub x: i32,
    pub y: i32,
}

/// Resize a window
#[derive(Message)]
#[rtype(result = "Result<(), DesktopError>")]
pub struct ResizeWindow {
    pub window_id: String,
    pub width: i32,
    pub height: i32,
}

/// Focus a window (bring to front)
#[derive(Message)]
#[rtype(result = "Result<(), DesktopError>")]
pub struct FocusWindow {
    pub window_id: String,
}

/// Get all windows
#[derive(Message)]
#[rtype(result = "Vec<shared_types::WindowState>")]
pub struct GetWindows;

/// Get current desktop state
#[derive(Message)]
#[rtype(result = "shared_types::DesktopState")]
pub struct GetDesktopState;

/// Register a new app
#[derive(Message)]
#[rtype(result = "Result<(), DesktopError>")]
pub struct RegisterApp {
    pub app: shared_types::AppDefinition,
}

/// Get all registered apps
#[derive(Message)]
#[rtype(result = "Vec<shared_types::AppDefinition>")]
pub struct GetApps;

/// Sync events (from EventStore)
#[derive(Message)]
#[rtype(result = "()")]
pub struct SyncEvents {
    pub events: Vec<shared_types::Event>,
}

/// Get actor info
#[derive(Message)]
#[rtype(result = "(String, String)")]
pub struct GetActorInfo;

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum DesktopError {
    #[error("Event store error: {0}")]
    EventStore(String),

    #[error("Window not found: {0}")]
    WindowNotFound(String),

    #[error("App not found: {0}")]
    AppNotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
}

// ============================================================================
// Handlers
// ============================================================================

impl Handler<OpenWindow> for DesktopActor {
    type Result = actix::ResponseActFuture<Self, Result<shared_types::WindowState, DesktopError>>;

    fn handle(&mut self, msg: OpenWindow, _ctx: &mut Context<Self>) -> Self::Result {
        // Check if app exists
        let app = match self.apps.get(&msg.app_id) {
            Some(app) => app.clone(),
            None => {
                return Box::pin(
                    async move { Err(DesktopError::AppNotFound(msg.app_id)) }.into_actor(self),
                );
            }
        };

        // Create window state
        let window_id = ulid::Ulid::new().to_string();
        let (x, y) = self.get_default_position(&msg.app_id);

        let window = shared_types::WindowState {
            id: window_id.clone(),
            app_id: msg.app_id.clone(),
            title: msg.title.clone(),
            x,
            y,
            width: app.default_width,
            height: app.default_height,
            z_index: self.next_z(),
            minimized: false,
            maximized: false,
            props: msg.props.unwrap_or_else(|| serde_json::json!({})),
        };

        // Store in memory
        self.windows.insert(window_id.clone(), window.clone());
        self.active_window = Some(window_id.clone());

        // Append event to EventStore and return window
        let payload = serde_json::to_value(&window).unwrap();
        let event_store = self.event_store.clone();
        let desktop_id = self.desktop_id.clone();
        let user_id = self.user_id.clone();
        let window_clone = window.clone();

        Box::pin(
            async move {
                if let Some(es) = event_store {
                    match es
                        .send(AppendEvent {
                            event_type: EVENT_WINDOW_OPENED.to_string(),
                            payload,
                            actor_id: desktop_id,
                            user_id,
                        })
                        .await
                    {
                        Ok(Ok(_)) => Ok(window_clone),
                        Ok(Err(e)) => Err(DesktopError::EventStore(e.to_string())),
                        Err(_) => Err(DesktopError::EventStore("Mailbox error".to_string())),
                    }
                } else {
                    Err(DesktopError::EventStore("No event store".to_string()))
                }
            }
            .into_actor(self),
        )
    }
}

impl Handler<CloseWindow> for DesktopActor {
    type Result = actix::ResponseActFuture<Self, Result<(), DesktopError>>;

    fn handle(&mut self, msg: CloseWindow, _ctx: &mut Context<Self>) -> Self::Result {
        // Remove from memory
        if self.windows.remove(&msg.window_id).is_none() {
            return Box::pin(
                async move { Err(DesktopError::WindowNotFound(msg.window_id)) }.into_actor(self),
            );
        }

        // Update active window
        if self.active_window.as_deref() == Some(&msg.window_id) {
            self.active_window = self.windows.keys().next().cloned();
        }

        // Append event
        let payload = serde_json::json!({"window_id": msg.window_id});
        self.append_event_unit(EVENT_WINDOW_CLOSED, payload)
    }
}

impl Handler<MoveWindow> for DesktopActor {
    type Result = actix::ResponseActFuture<Self, Result<(), DesktopError>>;

    fn handle(&mut self, msg: MoveWindow, _ctx: &mut Context<Self>) -> Self::Result {
        // Update memory
        if let Some(window) = self.windows.get_mut(&msg.window_id) {
            window.x = msg.x;
            window.y = msg.y;
        } else {
            return Box::pin(
                async move { Err(DesktopError::WindowNotFound(msg.window_id)) }.into_actor(self),
            );
        }

        // Append event
        let payload = serde_json::json!({
            "window_id": msg.window_id,
            "x": msg.x,
            "y": msg.y,
        });
        self.append_event_unit(EVENT_WINDOW_MOVED, payload)
    }
}

impl Handler<ResizeWindow> for DesktopActor {
    type Result = actix::ResponseActFuture<Self, Result<(), DesktopError>>;

    fn handle(&mut self, msg: ResizeWindow, _ctx: &mut Context<Self>) -> Self::Result {
        // Update memory
        if let Some(window) = self.windows.get_mut(&msg.window_id) {
            window.width = msg.width;
            window.height = msg.height;
        } else {
            return Box::pin(
                async move { Err(DesktopError::WindowNotFound(msg.window_id)) }.into_actor(self),
            );
        }

        // Append event
        let payload = serde_json::json!({
            "window_id": msg.window_id,
            "width": msg.width,
            "height": msg.height,
        });
        self.append_event_unit(EVENT_WINDOW_RESIZED, payload)
    }
}

impl Handler<FocusWindow> for DesktopActor {
    type Result = actix::ResponseActFuture<Self, Result<(), DesktopError>>;

    fn handle(&mut self, msg: FocusWindow, _ctx: &mut Context<Self>) -> Self::Result {
        // Check window exists
        if !self.windows.contains_key(&msg.window_id) {
            return Box::pin(
                async move { Err(DesktopError::WindowNotFound(msg.window_id)) }.into_actor(self),
            );
        }

        // Update state
        self.active_window = Some(msg.window_id.clone());
        // Get z-index first to avoid borrow issues
        let new_z = self.next_z();
        if let Some(window) = self.windows.get_mut(&msg.window_id) {
            window.z_index = new_z;
        }

        // Append event
        let payload = serde_json::json!({"window_id": msg.window_id});
        self.append_event_unit(EVENT_WINDOW_FOCUSED, payload)
    }
}

impl Handler<GetWindows> for DesktopActor {
    type Result = Vec<shared_types::WindowState>;

    fn handle(&mut self, _msg: GetWindows, _ctx: &mut Context<Self>) -> Self::Result {
        let mut windows: Vec<_> = self.windows.values().cloned().collect();
        // Sort by z-index
        windows.sort_by_key(|w| w.z_index);
        windows
    }
}

impl Handler<GetDesktopState> for DesktopActor {
    type Result = actix::ResponseActFuture<Self, shared_types::DesktopState>;

    fn handle(&mut self, _msg: GetDesktopState, _ctx: &mut Context<Self>) -> Self::Result {
        // If events haven't been synced yet, sync first
        if self.last_seq == 0 && self.event_store.is_some() {
            let event_store = self.event_store.clone().unwrap();
            let desktop_id = self.desktop_id.clone();

            Box::pin(
                async move {
                    // Load events from event store
                    let events_result = event_store
                        .send(GetEventsForActor {
                            actor_id: desktop_id,
                            since_seq: 0,
                        })
                        .await;

                    let events = match events_result {
                        Ok(Ok(events)) => events,
                        _ => Vec::new(),
                    };

                    (events,)
                }
                .into_actor(self)
                .map(|(events,), actor: &mut DesktopActor, _| {
                    // Project events to restore state
                    actor.project_events(events);

                    // Ensure default apps exist if none loaded
                    if actor.apps.is_empty() {
                        actor.apps.insert(
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

                    // Build and return state
                    let windows: Vec<_> = actor.windows.values().cloned().collect();
                    let active_window = actor.active_window.clone();
                    let apps: Vec<_> = actor.apps.values().cloned().collect();

                    shared_types::DesktopState {
                        windows,
                        active_window,
                        apps,
                    }
                }),
            )
        } else {
            // Already synced - return cached state immediately
            let windows: Vec<_> = self.windows.values().cloned().collect();
            let active_window = self.active_window.clone();
            let apps: Vec<_> = self.apps.values().cloned().collect();

            Box::pin(
                async move {
                    shared_types::DesktopState {
                        windows,
                        active_window,
                        apps,
                    }
                }
                .into_actor(self),
            )
        }
    }
}

impl Handler<RegisterApp> for DesktopActor {
    type Result = actix::ResponseActFuture<Self, Result<(), DesktopError>>;

    fn handle(&mut self, msg: RegisterApp, _ctx: &mut Context<Self>) -> Self::Result {
        // Store in memory
        self.apps.insert(msg.app.id.clone(), msg.app.clone());

        // Append event
        let payload = serde_json::to_value(&msg.app).unwrap();
        self.append_event_unit(EVENT_APP_REGISTERED, payload)
    }
}

impl Handler<GetApps> for DesktopActor {
    type Result = Vec<shared_types::AppDefinition>;

    fn handle(&mut self, _msg: GetApps, _ctx: &mut Context<Self>) -> Self::Result {
        self.apps.values().cloned().collect()
    }
}

impl Handler<SyncEvents> for DesktopActor {
    type Result = ();

    fn handle(&mut self, msg: SyncEvents, _ctx: &mut Context<Self>) {
        self.project_events(msg.events);
    }
}

impl Handler<GetActorInfo> for DesktopActor {
    type Result = actix::ResponseActFuture<Self, (String, String)>;

    fn handle(&mut self, _msg: GetActorInfo, _ctx: &mut Context<Self>) -> Self::Result {
        let desktop_id = self.desktop_id.clone();
        let user_id = self.user_id.clone();
        Box::pin(async move { (desktop_id, user_id) }.into_actor(self))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use actix::Actor;

    // ============================================================================
    // Test 1: Opening window creates it with proper defaults
    // ============================================================================

    #[actix::test]
    async fn test_open_window_creates_window() {
        let event_store = EventStoreActor::new_in_memory().await.unwrap().start();
        let desktop =
            DesktopActor::new("desktop-1".to_string(), "user-1".to_string(), event_store).start();

        // Register an app first
        let _ = desktop
            .send(RegisterApp {
                app: shared_types::AppDefinition {
                    id: "chat".to_string(),
                    name: "Chat".to_string(),
                    icon: "💬".to_string(),
                    component_code: "ChatApp".to_string(),
                    default_width: 800,
                    default_height: 600,
                },
            })
            .await
            .unwrap();

        // Open a window
        let window = desktop
            .send(OpenWindow {
                app_id: "chat".to_string(),
                title: "Chat Window".to_string(),
                props: None,
            })
            .await
            .unwrap();

        assert!(window.is_ok());
        let window = window.unwrap();
        assert_eq!(window.app_id, "chat");
        assert_eq!(window.title, "Chat Window");
        assert_eq!(window.width, 800);
        assert_eq!(window.height, 600);
        assert!(!window.minimized);
    }

    // ============================================================================
    // Test 2: Opening window for non-existent app fails
    // ============================================================================

    #[actix::test]
    async fn test_open_window_unknown_app_fails() {
        let event_store = EventStoreActor::new_in_memory().await.unwrap().start();
        let desktop =
            DesktopActor::new("desktop-1".to_string(), "user-1".to_string(), event_store).start();

        // Try to open window for unknown app
        let result = desktop
            .send(OpenWindow {
                app_id: "unknown".to_string(),
                title: "Unknown".to_string(),
                props: None,
            })
            .await;

        assert!(result.is_ok()); // Mailbox OK
        let inner = result.unwrap();
        assert!(inner.is_err()); // Handler returned error
    }

    // ============================================================================
    // Test 3: Closing window removes it
    // ============================================================================

    #[actix::test]
    async fn test_close_window_removes_it() {
        let event_store = EventStoreActor::new_in_memory().await.unwrap().start();
        let desktop =
            DesktopActor::new("desktop-1".to_string(), "user-1".to_string(), event_store).start();

        // Register app and open window
        let _ = desktop
            .send(RegisterApp {
                app: shared_types::AppDefinition {
                    id: "chat".to_string(),
                    name: "Chat".to_string(),
                    icon: "💬".to_string(),
                    component_code: "ChatApp".to_string(),
                    default_width: 800,
                    default_height: 600,
                },
            })
            .await
            .unwrap();

        let window = desktop
            .send(OpenWindow {
                app_id: "chat".to_string(),
                title: "Chat".to_string(),
                props: None,
            })
            .await
            .unwrap()
            .unwrap();

        let window_id = window.id;

        // Close the window
        let result = desktop
            .send(CloseWindow {
                window_id: window_id.clone(),
            })
            .await
            .unwrap();

        assert!(result.is_ok());

        // Verify window is gone
        let windows = desktop.send(GetWindows).await.unwrap();
        assert!(windows.is_empty());
    }

    // ============================================================================
    // Test 4: Moving window updates position
    // ============================================================================

    #[actix::test]
    async fn test_move_window_updates_position() {
        let event_store = EventStoreActor::new_in_memory().await.unwrap().start();
        let desktop =
            DesktopActor::new("desktop-1".to_string(), "user-1".to_string(), event_store).start();

        // Register app and open window
        let _ = desktop
            .send(RegisterApp {
                app: shared_types::AppDefinition {
                    id: "chat".to_string(),
                    name: "Chat".to_string(),
                    icon: "💬".to_string(),
                    component_code: "ChatApp".to_string(),
                    default_width: 800,
                    default_height: 600,
                },
            })
            .await
            .unwrap();

        let window = desktop
            .send(OpenWindow {
                app_id: "chat".to_string(),
                title: "Chat".to_string(),
                props: None,
            })
            .await
            .unwrap()
            .unwrap();

        let window_id = window.id;

        // Move the window
        let result = desktop
            .send(MoveWindow {
                window_id: window_id.clone(),
                x: 200,
                y: 300,
            })
            .await
            .unwrap();

        assert!(result.is_ok());

        // Verify position updated
        let windows = desktop.send(GetWindows).await.unwrap();
        assert_eq!(windows[0].x, 200);
        assert_eq!(windows[0].y, 300);
    }

    // ============================================================================
    // Test 5: Focus window brings it to front (highest z-index)
    // ============================================================================

    #[actix::test]
    async fn test_focus_window_brings_to_front() {
        let event_store = EventStoreActor::new_in_memory().await.unwrap().start();
        let desktop =
            DesktopActor::new("desktop-1".to_string(), "user-1".to_string(), event_store).start();

        // Register app and open two windows
        let _ = desktop
            .send(RegisterApp {
                app: shared_types::AppDefinition {
                    id: "chat".to_string(),
                    name: "Chat".to_string(),
                    icon: "💬".to_string(),
                    component_code: "ChatApp".to_string(),
                    default_width: 800,
                    default_height: 600,
                },
            })
            .await
            .unwrap();

        let window1 = desktop
            .send(OpenWindow {
                app_id: "chat".to_string(),
                title: "Window 1".to_string(),
                props: None,
            })
            .await
            .unwrap()
            .unwrap();

        let window2 = desktop
            .send(OpenWindow {
                app_id: "chat".to_string(),
                title: "Window 2".to_string(),
                props: None,
            })
            .await
            .unwrap()
            .unwrap();

        // Window 2 should have higher z-index
        assert!(window2.z_index > window1.z_index);

        // Focus window 1
        let _ = desktop
            .send(FocusWindow {
                window_id: window1.id.clone(),
            })
            .await
            .unwrap();

        // Window 1 should now have higher z-index than window 2 had
        let windows = desktop.send(GetWindows).await.unwrap();
        let w1 = windows.iter().find(|w| w.id == window1.id).unwrap();
        assert!(w1.z_index > window2.z_index);
    }

    // ============================================================================
    // Test 6: Get desktop state returns all windows and apps
    // ============================================================================

    #[actix::test]
    async fn test_get_desktop_state() {
        let event_store = EventStoreActor::new_in_memory().await.unwrap().start();
        let desktop =
            DesktopActor::new("desktop-1".to_string(), "user-1".to_string(), event_store).start();

        // Register app
        let _ = desktop
            .send(RegisterApp {
                app: shared_types::AppDefinition {
                    id: "chat".to_string(),
                    name: "Chat".to_string(),
                    icon: "💬".to_string(),
                    component_code: "ChatApp".to_string(),
                    default_width: 800,
                    default_height: 600,
                },
            })
            .await
            .unwrap();

        // Open window
        let window = desktop
            .send(OpenWindow {
                app_id: "chat".to_string(),
                title: "Chat".to_string(),
                props: None,
            })
            .await
            .unwrap()
            .unwrap();

        // Get desktop state
        let state = desktop.send(GetDesktopState).await.unwrap();

        assert_eq!(state.windows.len(), 1);
        assert_eq!(state.apps.len(), 1);
        assert_eq!(state.active_window, Some(window.id));
    }

    // ============================================================================
    // Test 7: Registering app adds it to registry
    // ============================================================================

    #[actix::test]
    async fn test_register_app() {
        let event_store = EventStoreActor::new_in_memory().await.unwrap().start();
        let desktop =
            DesktopActor::new("desktop-1".to_string(), "user-1".to_string(), event_store).start();

        // Register a new app (chat app is added by default on startup)
        let result = desktop
            .send(RegisterApp {
                app: shared_types::AppDefinition {
                    id: "calc".to_string(),
                    name: "Calculator".to_string(),
                    icon: "🧮".to_string(),
                    component_code: "CalcApp".to_string(),
                    default_width: 300,
                    default_height: 400,
                },
            })
            .await
            .unwrap();

        assert!(result.is_ok());

        let apps = desktop.send(GetApps).await.unwrap();
        // Should have 2 apps: default chat + registered calc
        assert_eq!(apps.len(), 2);
        assert!(apps.iter().any(|a| a.id == "chat"));
        assert!(apps.iter().any(|a| a.id == "calc"));
    }
}
