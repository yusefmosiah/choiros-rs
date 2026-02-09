//! Runtime environment helpers shared by app startup and live integration tests.

use std::path::Path;
use std::sync::OnceLock;

static TLS_CERT_PATH: OnceLock<Option<String>> = OnceLock::new();

/// Ensure TLS cert path env vars are set for rustls/hyper clients.
///
/// This avoids platform-cert initialization edge cases in some local/dev
/// environments by preferring an explicit CA bundle path.
pub fn ensure_tls_cert_env() -> Option<String> {
    TLS_CERT_PATH
        .get_or_init(|| {
            if let Ok(existing) = std::env::var("SSL_CERT_FILE") {
                if !existing.trim().is_empty() {
                    return Some(existing);
                }
            }

            if let Ok(nix_bundle) = std::env::var("NIX_SSL_CERT_FILE") {
                if !nix_bundle.trim().is_empty() && Path::new(&nix_bundle).exists() {
                    std::env::set_var("SSL_CERT_FILE", &nix_bundle);
                    return Some(nix_bundle);
                }
            }

            let candidates = [
                "/etc/ssl/cert.pem",                  // macOS default
                "/etc/ssl/certs/ca-certificates.crt", // Debian/Ubuntu
                "/etc/pki/tls/certs/ca-bundle.crt",   // RHEL/CentOS/Fedora
                "/etc/ssl/certs/ca-bundle.crt",       // Alpine/openSUSE variants
            ];

            for candidate in candidates {
                if Path::new(candidate).exists() {
                    std::env::set_var("SSL_CERT_FILE", candidate);
                    return Some(candidate.to_string());
                }
            }

            None
        })
        .clone()
}
