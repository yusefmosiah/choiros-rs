mod actors;
mod api;
mod actor_manager;

use actix::Actor;
use actix_cors::Cors;
use actix_web::{http::header, web, App, HttpServer};
use actors::{EventStoreActor, AppendEvent};
use actor_manager::AppState;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    tracing::info!("Starting ChoirOS Sandbox API Server");

    // Use configurable path for database
    let db_path = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "/opt/choiros/data/events.db".to_string());
    let db_path = std::path::PathBuf::from(db_path);
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create data directory");
    }

    // Create EventStoreActor (foundation of the system)
    // libsql takes a plain file path (not sqlite:// URL like sqlx)
    let db_path_str = db_path.to_str().expect("Invalid database path");
    tracing::info!("Connecting to database: {}", db_path_str);
    let event_store = EventStoreActor::new(db_path_str)
        .await
        .expect("Failed to create event store")
        .start();

    tracing::info!("EventStoreActor started");

    // Log startup event
    let event = event_store.send(AppendEvent {
        event_type: "system.startup".to_string(),
        payload: serde_json::json!({"version": "0.1.0"}),
        actor_id: "system".to_string(),
        user_id: "system".to_string(),
    }).await;

    match event {
        Ok(Ok(evt)) => tracing::info!(seq = evt.seq, "Startup event logged"),
        Ok(Err(e)) => tracing::error!("Failed to log startup: {}", e),
        Err(e) => tracing::error!("Mailbox error: {}", e),
    }

    // Create app state with actor manager
    let app_state = web::Data::new(AppState::new(event_store.clone()));

    // Create WebSocket sessions state
    let ws_sessions = web::Data::new(api::websocket::WsSessions::default());

    tracing::info!("Starting HTTP server on http://0.0.0.0:8080");

    // Start HTTP server with CORS
    HttpServer::new(move || {
        // Configure CORS to allow known UI origins
        let cors = Cors::default()
            .allowed_origin("http://13.218.213.227")
            .allowed_origin("http://choir.chat")
            .allowed_origin("https://choir.chat")
            .allowed_origin("http://localhost:3000")
            .allowed_origin("http://127.0.0.1:3000")
            .allowed_methods(vec!["GET", "POST", "DELETE", "PATCH", "OPTIONS"])
            .allowed_headers(vec![header::CONTENT_TYPE, header::ACCEPT, header::AUTHORIZATION])
            .max_age(3600);

        App::new()
            .wrap(cors)
            .app_data(app_state.clone())
            .app_data(ws_sessions.clone())
            .route("/health", web::get().to(api::health_check))
            .route("/ws", web::get().to(api::websocket::ws_handler))
            .configure(api::config)
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
