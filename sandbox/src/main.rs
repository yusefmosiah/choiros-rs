use axum::http::{header, HeaderValue, Method};
use ractor::Actor;
use sandbox::actors::event_store::{
    AppendEvent, EventStoreActor, EventStoreArguments, EventStoreMsg,
};
use sandbox::api;
use sandbox::app_state::AppState;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::{AllowOrigin, CorsLayer};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    tracing::info!("Starting ChoirOS Sandbox API Server");

    // Use configurable path for database
    let db_path =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "/opt/choiros/data/events.db".to_string());
    let db_path = std::path::PathBuf::from(db_path);
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create data directory");
    }

    // Create EventStoreActor (foundation of the system)
    // libsql takes a plain file path (not sqlite:// URL like sqlx)
    let db_path_str = db_path.to_str().expect("Invalid database path");
    tracing::info!("Connecting to database: {}", db_path_str);
    let (event_store, _handle) = Actor::spawn(
        None,
        EventStoreActor,
        EventStoreArguments::File(db_path_str.to_string()),
    )
    .await
    .expect("Failed to create event store");

    tracing::info!("EventStoreActor started");

    // Log startup event
    let startup_event = AppendEvent {
        event_type: "system.startup".to_string(),
        payload: serde_json::json!({"version": "0.1.0"}),
        actor_id: "system".to_string(),
        user_id: "system".to_string(),
    };

    let event_result = ractor::call!(event_store, |reply| EventStoreMsg::Append {
        event: startup_event,
        reply,
    });

    match event_result {
        Ok(Ok(evt)) => tracing::info!(seq = evt.seq, "Startup event logged"),
        Ok(Err(e)) => tracing::error!("Failed to log startup: {}", e),
        Err(e) => tracing::error!("RPC error: {}", e),
    }

    // Create WebSocket sessions state
    let ws_sessions: api::websocket::WsSessions = Arc::new(tokio::sync::Mutex::new(HashMap::new()));

    let app_state = Arc::new(AppState::new(event_store.clone()));
    let _ = app_state
        .ensure_supervisor()
        .await
        .expect("Failed to spawn ApplicationSupervisor");

    tracing::info!("Starting HTTP server on http://0.0.0.0:8080");

    // Configure CORS to allow known UI origins
    let allowed_origins = [
        "http://13.218.213.227",
        "http://choir.chat",
        "https://choir.chat",
        "http://localhost:3000",
        "http://127.0.0.1:3000",
    ]
    .iter()
    .map(|origin| HeaderValue::from_str(origin).expect("Invalid CORS origin"))
    .collect::<Vec<_>>();

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed_origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::DELETE,
            Method::PATCH,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::ACCEPT, header::AUTHORIZATION])
        .max_age(std::time::Duration::from_secs(3600));

    let api_state = api::ApiState {
        app_state,
        ws_sessions,
    };

    let app = api::router().with_state(api_state).layer(cors);

    let listener = TcpListener::bind("0.0.0.0:8080").await?;
    axum::serve(listener, app).await
}
