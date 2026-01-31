mod actors;
mod api;

use actix::Actor;
use actix_web::{web, App, HttpServer};
use actors::{EventStoreActor, AppendEvent, GetEventsForActor};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    tracing::info!("Starting ChoirOS Sandbox API Server");
    
    // Create EventStoreActor (foundation of the system)
    let event_store = EventStoreActor::new("/home/ubuntu/choiros-rs/data/events.db")
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
