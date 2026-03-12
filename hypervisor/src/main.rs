mod api;
mod auth;
mod config;
mod db;
mod jobs;
mod middleware;
mod provider_gateway;
mod proxy;
mod runtime_registry;
mod sandbox;
mod session_store;
mod state;

use std::sync::Arc;

use axum::{
    middleware as axum_middleware,
    routing::{any, delete, get, post, put},
    Router,
};
use tower_http::trace::TraceLayer;
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
        config.sandbox_runtime_ctl.clone(),
        config.sandbox_live_port,
        config.sandbox_dev_port,
        config.sandbox_branch_port_start,
        config.sandbox_branch_port_end,
        config.sandbox_idle_timeout,
        config.provider_gateway_base_url.clone(),
        config.provider_gateway_token.clone(),
        config.machine_classes.clone(),
    );

    // Boot live sandbox in background so the HTTP server starts immediately.
    // The VM takes ~90s to boot; requests will get 503 until it's ready.
    {
        let reg = Arc::clone(&sandbox_registry);
        tokio::spawn(async move { reg.boot_live_sandbox().await });
    }

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
            rate_limit_state: Arc::new(dashmap::DashMap::new()),
        },
        proxy_client: proxy::new_pooled_client(),
    });

    let app = Router::new()
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
        // Root shell page (Dioxus SPA bootstrap). Runtime APIs still proxy via fallback.
        .route("/", get(auth::handlers::login_page))
        // Public auth shell pages.
        .route("/login", get(auth::handlers::login_page))
        .route("/register", get(auth::handlers::register_page))
        .route("/recovery", get(auth::handlers::recovery_page))
        .route(
            "/provider/v1/{provider}/{*rest}",
            any(provider_gateway::forward_provider_request),
        )
        // Profile — user-facing machine class preference (ADR-0014 Phase 6)
        .route(
            "/profile/machine-class",
            get(auth::handlers::get_profile_machine_class),
        )
        .route(
            "/profile/machine-class",
            put(auth::handlers::set_profile_machine_class),
        )
        // Heartbeat — keeps sandbox alive without proxying
        .route("/heartbeat", post(api::heartbeat))
        // Admin sandbox management
        .route("/admin/stats", get(api::host_stats))
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
            "/admin/sandboxes/{user_id}/{role}/hibernate",
            post(api::hibernate_sandbox),
        )
        .route(
            "/admin/sandboxes/{user_id}/swap",
            post(api::swap_sandbox_roles),
        )
        .route(
            "/admin/sandboxes/{user_id}/branches/{branch}/start",
            post(api::start_branch_sandbox),
        )
        .route(
            "/admin/sandboxes/{user_id}/branches/{branch}/stop",
            post(api::stop_branch_sandbox),
        )
        // Machine class management (ADR-0014 Phase 6)
        .route("/admin/machine-classes", get(api::list_machine_classes))
        .route(
            "/admin/sandboxes/{user_id}/machine-class",
            put(api::set_machine_class),
        )
        .route(
            "/admin/sandboxes/{user_id}/machine-class",
            delete(api::clear_machine_class),
        )
        .route(
            "/admin/sandboxes/{user_id}/pointers",
            get(api::list_route_pointers),
        )
        .route(
            "/admin/sandboxes/{user_id}/pointers/set",
            post(api::set_route_pointer),
        )
        // Job queue (ADR-0014 Phase 7)
        .route("/admin/jobs", post(api::create_job))
        .route("/admin/jobs", get(api::list_jobs))
        .route("/admin/jobs/{job_id}", get(api::get_job))
        .route("/admin/jobs/{job_id}", delete(api::cancel_job))
        // Promotion API (ADR-0014 Phase 8)
        .route(
            "/admin/sandboxes/{user_id}/promote",
            post(api::promote_sandbox),
        )
        .route(
            "/admin/sandboxes/{user_id}/promotions",
            get(api::list_promotions),
        )
        .route("/admin/promotions/{promotion_id}", get(api::get_promotion))
        // All non-auth traffic is routed to sandbox runtime (auth enforced by middleware)
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
