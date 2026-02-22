use std::sync::Arc;

use sqlx::SqlitePool;
use webauthn_rs::prelude::Webauthn;

use crate::sandbox::SandboxRegistry;

#[derive(Clone)]
pub struct ProviderGatewayState {
    pub token: Option<String>,
    pub allowed_upstreams: Vec<String>,
    pub client: reqwest::Client,
}

pub struct AppState {
    pub db: SqlitePool,
    pub webauthn: Arc<Webauthn>,
    pub sandbox_registry: Arc<SandboxRegistry>,
    pub provider_gateway: ProviderGatewayState,
}
