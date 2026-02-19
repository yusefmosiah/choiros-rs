use std::sync::Arc;

use sqlx::SqlitePool;
use webauthn_rs::prelude::Webauthn;

use crate::sandbox::SandboxRegistry;

pub struct AppState {
    pub db: SqlitePool,
    pub webauthn: Arc<Webauthn>,
    pub sandbox_registry: Arc<SandboxRegistry>,
}
