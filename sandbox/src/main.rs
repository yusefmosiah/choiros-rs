mod actors;

use actix::{Actor, System};
use actors::{EventStoreActor, AppendEvent, GetEventsForActor};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    tracing::info!("Starting ChoirOS Sandbox");
    
    // Create EventStoreActor (foundation of the system)
    let event_store = EventStoreActor::new("./data/events.db")
        .await
        .expect("Failed to create event store")
        .start();
    
    tracing::info!("EventStoreActor started");
    
    // Test: Append an event
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
    
    // Test: Retrieve events
    let events = event_store.send(GetEventsForActor {
        actor_id: "system".to_string(),
        since_seq: 0,
    }).await;
    
    match events {
        Ok(Ok(evts)) => tracing::info!(count = evts.len(), "Retrieved events"),
        Ok(Err(e)) => tracing::error!("Failed to retrieve: {}", e),
        Err(e) => tracing::error!("Mailbox error: {}", e),
    }
    
    tracing::info!("Sandbox initialized successfully!");
    tracing::info!("Event store is ready for actors.");
    
    // Keep running (in real implementation, this would be the Actix web server)
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
    }
}
