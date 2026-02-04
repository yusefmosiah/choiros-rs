mod actors;
mod api;

use actix_web::{web, App, HttpServer};
use ractor::Actor;

use crate::actors::event_store::{EventStoreActor, EventStoreArguments, EventStoreMsg, append_event};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    tracing::info!("Starting ChoirOS Sandbox API Server");
    
    // Create EventStoreActor using ractor
    let (event_store, _handle) = Actor::spawn(
        None,
        EventStoreActor,
        EventStoreArguments::File("/home/ubuntu/choiros-rs/data/events.db".to_string()),
    )
    .await
    .expect("Failed to create event store");
    
    tracing::info!("EventStoreActor started");
    
    // Log startup event using ractor
    let startup_result = append_event(
        &event_store,
        crate::actors::event_store::AppendEvent {
            event_type: "system.startup".to_string(),
            payload: serde_json::json!({"version": "0.1.0"}),
            actor_id: "system".to_string(),
            user_id: "system".to_string(),
        },
    ).await;
    
    match startup_result {
        Ok(Ok(evt)) => tracing::info!(seq = evt.seq, "Startup event logged"),
        Ok(Err(e)) => tracing::error!("Failed to log startup: {}", e),
        Err(e) => tracing::error!("Actor error: {}", e),
    }
    
    // Clone for HTTP server
    let event_store_data = web::Data::new(event_store.clone());
    
    tracing::info!("Starting HTTP server on http://0.0.0.0:8080");
    
    // Start HTTP server
    HttpServer::new(move || {
        App::new()
            .app_data(event_store_data.clone())
            .route("/health", web::get().to(api::health_check))
            .configure(api::config)
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
