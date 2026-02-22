mod api;
mod auth;
mod config;
mod db;
mod middleware;
mod provider_gateway;
mod proxy;
mod sandbox;
mod session_store;
mod state;

use std::sync::Arc;

use axum::{
    middleware as axum_middleware,
    routing::{any, get, post},
    Router,
};
use tower_http::{services::ServeDir, trace::TraceLayer};
use tower_sessions::{Expiry, SessionManagerLayer};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hypervisor=debug,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = config::Config::from_env()?;
    info!(port = config.port, "hypervisor starting");

    // Database
    let db = db::connect(&config.database_url).await?;

    // Session store — SQLite-backed, sessions survive hypervisor restarts.
    let session_store = session_store::SqliteSessionStore::new(db.clone());
    session_store
        .migrate()
        .await
        .map_err(|e| anyhow::anyhow!("session store migration failed: {e}"))?;

    // Spawn expired-session cleanup every hour.
    tokio::spawn(session_store::run_expired_session_cleanup(
        session_store.clone(),
        std::time::Duration::from_secs(3600),
    ));

    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false) // set true in prod (HTTPS only)
        .with_expiry(Expiry::OnInactivity(time::Duration::hours(24)));

    // WebAuthn
    let webauthn = auth::build_webauthn(&config)?;

    // Sandbox registry
    let sandbox_registry = sandbox::SandboxRegistry::new(
        config.sandbox_binary.clone(),
        config.sandbox_runtime,
        config.sandbox_live_port,
        config.sandbox_dev_port,
        config.sandbox_idle_timeout,
        config.provider_gateway_base_url.clone(),
        config.provider_gateway_token.clone(),
    );

    // Spawn idle watchdog
    {
        let reg = Arc::clone(&sandbox_registry);
        tokio::spawn(reg.run_idle_watchdog());
    }

    let state = Arc::new(AppState {
        db,
        webauthn,
        sandbox_registry,
        provider_gateway: state::ProviderGatewayState {
            token: config.provider_gateway_token.clone(),
            base_url: config.provider_gateway_base_url.clone(),
            allowed_upstreams: config.provider_gateway_allowed_upstreams.clone(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()?,
            rate_limit_per_minute: config.provider_gateway_rate_limit_per_minute,
            rate_limit_state: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        },
    });

    // Dioxus WASM frontend — served from the dx build output directory.
    // Default matches `dx build` debug output. Override with FRONTEND_DIST in prod.
    // Release path: dioxus-desktop/target/dx/dioxus-desktop/release/web/public
    let frontend_dist = config::frontend_dist_from_env();
    info!(path = %frontend_dist, "serving frontend assets from");

    let app = Router::new()
        // Auth pages — serve the Dioxus index.html; the WASM router handles
        // /login, /register, /recovery client-side.
        // Falls through to the ServeDir nest below when the dist exists.
        .route("/", get(auth::handlers::root_page))
        .route("/login", get(auth::handlers::login_page))
        .route("/register", get(auth::handlers::register_page))
        .route("/recovery", get(auth::handlers::recovery_page))
        // Auth API endpoints
        .route("/auth/register/start", post(auth::handlers::register_start))
        .route(
            "/auth/register/finish",
            post(auth::handlers::register_finish),
        )
        .route("/auth/login/start", post(auth::handlers::login_start))
        .route("/auth/login/finish", post(auth::handlers::login_finish))
        .route("/auth/logout", post(auth::handlers::logout))
        .route("/auth/recovery", post(auth::handlers::recovery))
        .route("/auth/me", get(auth::handlers::me))
        .route(
            "/provider/v1/{provider}/{*rest}",
            any(provider_gateway::forward_provider_request),
        )
        // Admin sandbox management
        .route("/admin/sandboxes", get(api::list_sandboxes))
        .route(
            "/admin/sandboxes/{user_id}/{role}/start",
            post(api::start_sandbox),
        )
        .route(
            "/admin/sandboxes/{user_id}/{role}/stop",
            post(api::stop_sandbox),
        )
        .route(
            "/admin/sandboxes/{user_id}/swap",
            post(api::swap_sandbox_roles),
        )
        // Frontend static assets — served without auth.
        // /wasm/*   — WASM binary + generated JS bindings + snippets
        // /assets/* — hashed CSS/font assets (manganis output)
        // Other public root files (xterm.js etc.) are only needed after the
        // user is authenticated and proxied to the sandbox, which serves them.
        .nest_service("/wasm", ServeDir::new(format!("{frontend_dist}/wasm")))
        .nest_service("/assets", ServeDir::new(format!("{frontend_dist}/assets")))
        // All other traffic → proxy to sandbox (auth enforced by middleware)
        .fallback(middleware::proxy_to_sandbox)
        .layer(axum_middleware::from_fn_with_state(
            Arc::clone(&state),
            middleware::require_auth,
        ))
        .layer(session_layer)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", config.port);
    info!("listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
