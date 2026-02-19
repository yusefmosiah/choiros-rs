use std::sync::Arc;

use url::Url;
use webauthn_rs::prelude::*;

use crate::config::Config;

pub mod handlers;
pub mod session;

/// Shared WebAuthn instance (cheap to clone — Arc-wrapped internally).
pub fn build_webauthn(config: &Config) -> anyhow::Result<Arc<Webauthn>> {
    let rp_origin = Url::parse(&config.webauthn_rp_origin)
        .map_err(|e| anyhow::anyhow!("invalid WEBAUTHN_RP_ORIGIN: {e}"))?;

    let webauthn = WebauthnBuilder::new(&config.webauthn_rp_id, &rp_origin)
        .map_err(|e| anyhow::anyhow!("WebauthnBuilder error: {e}"))?
        .rp_name(&config.webauthn_rp_name)
        .build()
        .map_err(|e| anyhow::anyhow!("Webauthn build error: {e}"))?;

    Ok(Arc::new(webauthn))
}

/// Number of recovery codes issued per user.
pub const RECOVERY_CODE_COUNT: usize = 10;

/// Generate `RECOVERY_CODE_COUNT` plaintext recovery codes.
/// Returns `(plaintext_codes, argon2id_hashes)`.
pub fn generate_recovery_codes() -> anyhow::Result<(Vec<String>, Vec<String>)> {
    use argon2::{
        password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
        Argon2,
    };

    let argon2 = Argon2::default();
    let mut plaintexts = Vec::with_capacity(RECOVERY_CODE_COUNT);
    let mut hashes = Vec::with_capacity(RECOVERY_CODE_COUNT);

    for _ in 0..RECOVERY_CODE_COUNT {
        // 10 random bytes → 20 hex chars → grouped as XXXXX-XXXXX-XXXXX-XXXXX
        use rand::RngCore;
        let mut raw = [0u8; 10];
        rand::rng().fill_bytes(&mut raw);
        let hex = hex::encode(raw);
        let code = format!(
            "{}-{}-{}-{}",
            &hex[0..5],
            &hex[5..10],
            &hex[10..15],
            &hex[15..20]
        );
        let salt = SaltString::generate(&mut OsRng);
        let hash = argon2
            .hash_password(code.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("argon2 hash error: {e}"))?
            .to_string();
        plaintexts.push(code);
        hashes.push(hash);
    }

    Ok((plaintexts, hashes))
}

/// Verify a recovery code candidate against a stored argon2id hash.
pub fn verify_recovery_code(candidate: &str, hash: &str) -> bool {
    use argon2::{
        password_hash::{PasswordHash, PasswordVerifier},
        Argon2,
    };

    let parsed = match PasswordHash::new(hash) {
        Ok(h) => h,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(candidate.as_bytes(), &parsed)
        .is_ok()
}
