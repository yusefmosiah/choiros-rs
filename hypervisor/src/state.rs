use std::sync::Arc;
use std::{collections::HashMap, time::Instant};

use tokio::sync::Mutex;

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
    pub rate_limit_state: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
}

pub struct AppState {
    pub db: SqlitePool,
    pub webauthn: Arc<Webauthn>,
    pub sandbox_registry: Arc<SandboxRegistry>,
    pub provider_gateway: ProviderGatewayState,
}
