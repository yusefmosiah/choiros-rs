use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use sqlx::SqlitePool;
use webauthn_rs::prelude::Webauthn;

use crate::sandbox::SandboxRegistry;

#[derive(Clone)]
pub struct ProviderGatewayState {
    pub token: Option<String>,
    pub base_url: Option<String>,
    pub allowed_upstreams: Vec<String>,
    pub client: reqwest::Client,
    pub rate_limit_per_minute: usize,
    /// ADR-0022: DashMap for per-sandbox rate limit concurrency.
    pub rate_limit_state: Arc<DashMap<String, Vec<Instant>>>,
}

pub struct AppState {
    pub db: SqlitePool,
    pub webauthn: Arc<Webauthn>,
    pub sandbox_registry: Arc<SandboxRegistry>,
    pub provider_gateway: ProviderGatewayState,
    /// ADR-0022 Phase 5: connection-pooled HTTP client for sandbox proxy.
    pub proxy_client: crate::proxy::PooledClient,
}
