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

const FORBIDDEN_PROVIDER_KEY_ENVS: &[&str] = &[
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "ZAI_API_KEY",
    "KIMI_API_KEY",
    "GOOGLE_API_KEY",
    "MISTRAL_API_KEY",
    "AWS_BEARER_TOKEN_BEDROCK",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_ACCESS_KEY_ID",
    "AWS_SESSION_TOKEN",
];

fn load_env_file() {
    let cwd = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(e) => {
            tracing::warn!(error = %e, "Could not determine current directory for .env lookup");
            return;
        }
    };

    let mut current = cwd.clone();
    loop {
        let candidate = current.join(".env");
        if candidate.exists() {
            match dotenvy::from_path(&candidate) {
                Ok(_) => {
                    tracing::info!(path = %candidate.display(), "Loaded environment from .env");
                }
                Err(e) => {
                    tracing::warn!(
                        path = %candidate.display(),
                        error = %e,
                        "Failed to load .env file"
                    );
                }
            }
            return;
        }

        if !current.pop() {
            break;
        }
    }

    tracing::info!(
        cwd = %cwd.display(),
        "No .env file found in current directory or ancestors; using process environment only"
    );
}

fn assert_keyless_sandbox_env() -> std::io::Result<()> {
    if std::env::var("CHOIR_SANDBOX_KEYLESS_ENFORCED")
        .ok()
        .as_deref()
        != Some("true")
    {
        return Ok(());
    }

    for key in FORBIDDEN_PROVIDER_KEY_ENVS {
        if std::env::var(key).is_ok() {
            let message = format!(
                "Keyless sandbox policy violation: forbidden provider credential present in env: {key}"
            );
            tracing::error!("{message}");
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                message,
            ));
        }
    }

    if std::env::var("CHOIR_PROVIDER_GATEWAY_BASE_URL").is_err()
        || std::env::var("CHOIR_PROVIDER_GATEWAY_TOKEN").is_err()
    {
        let message =
            "Keyless sandbox policy requires CHOIR_PROVIDER_GATEWAY_BASE_URL and CHOIR_PROVIDER_GATEWAY_TOKEN";
        tracing::error!("{message}");
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            message,
        ));
    }

    Ok(())
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Load .env values early so provider/model keys are available to all actors.
    // Search the current directory and ancestors so running from `sandbox/` still
    // picks up repo-root `.env`.
    load_env_file();
    assert_keyless_sandbox_env()?;
    match sandbox::runtime_env::ensure_tls_cert_env() {
        Some(path) => tracing::info!(path = %path, "Configured SSL_CERT_FILE for TLS clients"),
        None => tracing::warn!(
            "No TLS cert bundle auto-detected; HTTPS provider calls may fail in this environment"
        ),
    }

    tracing::info!("Starting ChoirOS Sandbox API Server");

    // Use configurable path for database.
    // Strip the `sqlite:` URL scheme prefix if present — the sandbox uses sqlx
    // which accepts both `sqlite:path` and a bare path, but EventStoreArguments::File
    // expects a plain path without the scheme.
    let db_path_raw =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "/opt/choiros/data/events.db".to_string());
    let db_path_raw = db_path_raw
        .strip_prefix("sqlite:")
        .unwrap_or(&db_path_raw)
        .to_string();
    let db_path = std::path::PathBuf::from(&db_path_raw);
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create data directory");
    }

    // Create EventStoreActor (foundation of the system).
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
    api::websocket::spawn_writer_run_event_forwarder(event_store.clone(), ws_sessions.clone());

    let app_state = Arc::new(AppState::new(event_store.clone()));
    let _ = app_state
        .ensure_supervisor()
        .await
        .expect("Failed to spawn ApplicationSupervisor");

    // Watcher runtime is intentionally disabled during harness simplification.
    // Keep watcher code available for future reintroduction after control-flow refactor.
    tracing::info!("Watcher runtime disabled for simplification refactor");

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8080);

    tracing::info!("Starting HTTP server on http://0.0.0.0:{port}");

    // Configure CORS to allow known UI origins
    let allowed_origins = [
        "http://13.218.213.227",
        "http://choir-ip.com",
        "https://choir-ip.com",
        "http://localhost:3000",
        "http://127.0.0.1:3000",
        "http://100.91.73.16:3000",
        // Hypervisor reverse-proxy origin
        "http://localhost:9090",
        "http://127.0.0.1:9090",
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

    let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    axum::serve(listener, app).await
}
