use std::sync::{Arc, OnceLock};

use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use tower_sessions::Session;

use crate::{
    auth::session as sess,
    runtime_registry::{self, PointerTarget},
    sandbox::SandboxRole,
    AppState,
};

fn is_public_bootstrap_path(path: &str) -> bool {
    path == "/"
        || path == "/login"
        || path == "/register"
        || path == "/recovery"
        || path.starts_with("/wasm/")
        || path.starts_with("/assets/")
        || matches!(
            path,
            "/xterm.css"
                | "/xterm.js"
                | "/xterm-addon-fit.js"
                | "/terminal.js"
                | "/viewer-text.js"
                | "/favicon.ico"
        )
}

fn bootstrap_asset_rel_path(path: &str) -> Option<&str> {
    if path.starts_with("/assets/") || path.starts_with("/wasm/") {
        return path.strip_prefix('/');
    }

    match path {
        "/xterm.css"
        | "/xterm.js"
        | "/xterm-addon-fit.js"
        | "/terminal.js"
        | "/viewer-text.js"
        | "/favicon.ico" => path.strip_prefix('/'),
        _ => None,
    }
}

fn content_type_for_bootstrap_asset(path: &str) -> &'static str {
    if path.ends_with(".js") {
        "text/javascript; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".wasm") {
        "application/wasm"
    } else if path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".json") {
        "application/json"
    } else if path.ends_with(".ico") {
        "image/x-icon"
    } else {
        "application/octet-stream"
    }
}

async fn serve_public_bootstrap_asset(path: &str) -> Option<Response> {
    let rel = bootstrap_asset_rel_path(path)?;
    if rel.contains("..") {
        return Some((StatusCode::BAD_REQUEST, "invalid asset path").into_response());
    }

    let dist = crate::config::frontend_dist_from_env();
    let full_path = std::path::Path::new(&dist).join(rel);

    match tokio::fs::read(&full_path).await {
        Ok(bytes) => {
            let mut resp = Response::new(Body::from(bytes));
            *resp.status_mut() = StatusCode::OK;
            resp.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static(content_type_for_bootstrap_asset(rel)),
            );
            Some(resp)
        }
        Err(_) => Some((StatusCode::NOT_FOUND, "asset not found").into_response()),
    }
}

fn is_valid_branch_segment(branch: &str) -> bool {
    !branch.trim().is_empty()
        && branch.len() <= 64
        && branch
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

#[derive(Debug, Clone)]
enum SandboxRouteTarget {
    Role(SandboxRole),
    Branch(String),
}

#[derive(Debug, Clone)]
enum RouteTarget {
    Branch(String),
    Pointer(String),
}

#[derive(Debug, Clone)]
struct RouteResolution {
    target: RouteTarget,
    /// Prefix to strip before proxying to sandbox runtime.
    strip_prefix: Option<String>,
}

#[allow(clippy::result_large_err)]
fn resolve_route(path: &str) -> Result<RouteResolution, Response> {
    if path == "/dev" || path.starts_with("/dev/") {
        return Ok(RouteResolution {
            // /dev is a compatibility alias to pointer "dev".
            target: RouteTarget::Pointer("dev".to_string()),
            strip_prefix: Some("/dev".to_string()),
        });
    }

    if let Some(rest) = path.strip_prefix("/branch/") {
        let (branch, _) = match rest.split_once('/') {
            Some((branch, tail)) => (branch, Some(tail)),
            None => (rest, None),
        };

        if !is_valid_branch_segment(branch) {
            return Err((
                StatusCode::BAD_REQUEST,
                "invalid branch route segment (allowed: [A-Za-z0-9._-])",
            )
                .into_response());
        }

        return Ok(RouteResolution {
            target: RouteTarget::Branch(branch.to_string()),
            strip_prefix: Some(format!("/branch/{branch}")),
        });
    }

    Ok(RouteResolution {
        // Default routing follows pointer "main".
        target: RouteTarget::Pointer("main".to_string()),
        strip_prefix: None,
    })
}

fn strip_path_prefix(path_and_query: &str, prefix: &str) -> String {
    let stripped = path_and_query
        .strip_prefix(prefix)
        .unwrap_or(path_and_query);

    if stripped.is_empty() {
        "/".to_string()
    } else if stripped.starts_with('/') {
        stripped.to_string()
    } else {
        format!("/{stripped}")
    }
}

/// Middleware: require an authenticated session.
/// Unauthenticated requests to non-auth paths are redirected to /login.
/// Admin endpoints accept a bearer token from /run/choiros/admin.token (ADR-0020 Phase 0).
pub async fn require_auth(
    State(_state): State<Arc<AppState>>,
    session: Session,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path();

    // Allow auth endpoints and public app bootstrap assets without session.
    if path.starts_with("/auth/")
        || path.starts_with("/provider/v1/")
        || is_public_bootstrap_path(path)
    {
        return next.run(req).await;
    }

    // ADR-0020 Phase 0: Admin token auth for /admin/ endpoints.
    // The token is generated at boot and stored in /run/choiros/admin.token.
    // Machine-to-machine callers pass it as Authorization: Bearer <token>.
    if path.starts_with("/admin/") || path == "/health" {
        if let Some(auth_header) = req.headers().get("authorization") {
            if let Ok(auth_str) = auth_header.to_str() {
                if let Some(token) = auth_str.strip_prefix("Bearer ") {
                    if verify_admin_token(token) {
                        return next.run(req).await;
                    }
                }
            }
        }
    }

    // /health is always public (monitoring)
    if path == "/health" {
        return next.run(req).await;
    }

    if sess::get_user_id(&session).await.is_none() {
        return Redirect::to("/login").into_response();
    }

    next.run(req).await
}

/// Fallback handler: proxy authenticated traffic to the appropriate sandbox.
/// - `/dev/...` -> dev role runtime
/// - `/branch/<name>/...` -> branch runtime
/// - everything else -> live role runtime
pub async fn proxy_to_sandbox(
    State(state): State<Arc<AppState>>,
    session: Session,
    req: Request,
) -> Response {
    let path = req.uri().path().to_string();
    let is_public_bootstrap = is_public_bootstrap_path(&path);

    if is_public_bootstrap {
        if let Some(resp) = serve_public_bootstrap_asset(&path).await {
            return resp;
        }
    }

    let (user_id, authenticated) = match sess::get_user_id(&session).await {
        Some(id) => (id, true),
        None if is_public_bootstrap => ("public".to_string(), false),
        None => return (StatusCode::UNAUTHORIZED, "not authenticated").into_response(),
    };

    let resolution = match resolve_route(&path) {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    let (resolved_target, pointer_name) =
        match materialize_route_target(&state, &user_id, authenticated, resolution.target.clone())
            .await
        {
            Ok(v) => v,
            Err(resp) => return resp,
        };

    // Ensure the target runtime is running (auto-starts if needed).
    let port = match &resolved_target {
        SandboxRouteTarget::Role(role) => {
            match state.sandbox_registry.ensure_running(&user_id, *role).await {
                Ok(p) => p,
                Err(e) => {
                    let msg = format!("sandbox unavailable: {e}");
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        [("Retry-After", "30")],
                        msg,
                    )
                        .into_response();
                }
            }
        }
        SandboxRouteTarget::Branch(branch) => {
            match state
                .sandbox_registry
                .ensure_branch_running(&user_id, branch)
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    let msg = format!("branch sandbox unavailable: {e}");
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        [("Retry-After", "30")],
                        msg,
                    )
                        .into_response();
                }
            }
        }
    };

    // Detect WebSocket upgrade.
    let is_ws = req
        .headers()
        .get("upgrade")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);

    let req = sanitize_and_tag_proxy_request(
        req,
        Some(&user_id),
        &resolved_target,
        pointer_name.as_deref(),
        authenticated,
    );

    if is_ws {
        let path_with_query = req
            .uri()
            .path_and_query()
            .map(|pq| pq.as_str().to_string())
            .unwrap_or_else(|| path.clone());
        let path_to_proxy = match resolution.strip_prefix.as_deref() {
            Some(prefix) => strip_path_prefix(&path_with_query, prefix),
            None => path_with_query,
        };
        return crate::proxy::proxy_ws_raw(req, port, path_to_proxy).await;
    }

    if let Some(prefix) = resolution.strip_prefix.as_deref() {
        let (mut parts, body) = req.into_parts();
        let path_and_query = parts
            .uri
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or(parts.uri.path());
        let rewritten = strip_path_prefix(path_and_query, prefix);

        if let Ok(uri) = rewritten.parse::<axum::http::Uri>() {
            parts.uri = uri;
        }

        let req = Request::from_parts(parts, body);
        return crate::proxy::proxy_http(&state.proxy_client, req, port).await;
    }

    crate::proxy::proxy_http(&state.proxy_client, req, port).await
}

fn sanitize_and_tag_proxy_request(
    req: Request,
    user_id: Option<&str>,
    target: &SandboxRouteTarget,
    pointer_name: Option<&str>,
    authenticated: bool,
) -> Request {
    let (mut parts, body) = req.into_parts();

    // Never forward browser session credentials or client auth headers into sandbox.
    parts.headers.remove(header::COOKIE);
    parts.headers.remove(header::AUTHORIZATION);
    parts.headers.remove(header::PROXY_AUTHORIZATION);

    if let Some(user_id) = user_id {
        if let Ok(v) = HeaderValue::from_str(user_id) {
            parts.headers.insert("x-choiros-user-id", v);
        }
    }

    if let Some(pointer_name) = pointer_name {
        if let Ok(v) = HeaderValue::from_str(pointer_name) {
            parts.headers.insert("x-choiros-route-pointer", v);
        }
    }

    let (role_value, branch_value, runtime_label) = match target {
        SandboxRouteTarget::Role(SandboxRole::Live) => ("live", None, "live".to_string()),
        SandboxRouteTarget::Role(SandboxRole::Dev) => ("dev", None, "dev".to_string()),
        SandboxRouteTarget::Branch(branch) => {
            ("branch", Some(branch.as_str()), format!("branch:{branch}"))
        }
    };

    parts.headers.insert(
        "x-choiros-sandbox-role",
        HeaderValue::from_static(role_value),
    );

    if let Some(branch) = branch_value {
        if let Ok(v) = HeaderValue::from_str(branch) {
            parts.headers.insert("x-choiros-sandbox-branch", v);
        }
    }

    if let Ok(v) = HeaderValue::from_str(&runtime_label) {
        parts.headers.insert("x-choiros-sandbox-runtime", v);
    }

    parts.headers.insert(
        "x-choiros-proxy-authenticated",
        if authenticated {
            HeaderValue::from_static("true")
        } else {
            HeaderValue::from_static("false")
        },
    );

    Request::from_parts(parts, body)
}

async fn materialize_route_target(
    state: &Arc<AppState>,
    user_id: &str,
    authenticated: bool,
    target: RouteTarget,
) -> Result<(SandboxRouteTarget, Option<String>), Response> {
    let resolved = match target {
        RouteTarget::Branch(branch) => (SandboxRouteTarget::Branch(branch), None),
        RouteTarget::Pointer(pointer_name) => {
            // Public bootstrap requests route to live compatibility runtime without
            // pointer DB lookups, since there is no authenticated user context.
            if !authenticated {
                return Ok((
                    SandboxRouteTarget::Role(SandboxRole::Live),
                    Some(pointer_name),
                ));
            }

            if let Err(e) = runtime_registry::ensure_default_pointers(&state.db, user_id).await {
                return Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    format!("route pointer defaults unavailable: {e}"),
                )
                    .into_response());
            }

            let pointer_target =
                match runtime_registry::resolve_pointer_target(&state.db, user_id, &pointer_name)
                    .await
                {
                    Ok(Some(t)) => t,
                    Ok(None) => {
                        return Err((
                            StatusCode::NOT_FOUND,
                            format!("route pointer not found: {pointer_name}"),
                        )
                            .into_response())
                    }
                    Err(e) => {
                        return Err((
                            StatusCode::SERVICE_UNAVAILABLE,
                            format!("route pointer resolution unavailable: {e}"),
                        )
                            .into_response())
                    }
                };

            let target = match pointer_target {
                PointerTarget::Role(role) => SandboxRouteTarget::Role(role),
                PointerTarget::Branch(branch) => SandboxRouteTarget::Branch(branch),
            };
            (target, Some(pointer_name))
        }
    };

    Ok(resolved)
}

/// ADR-0020 Phase 0: Verify admin bearer token against /run/choiros/admin.token.
/// The token is generated at boot by a systemd oneshot and is only readable by root.
/// This enables machine-to-machine admin operations without WebAuthn.
fn verify_admin_token(token: &str) -> bool {
    static ADMIN_TOKEN: OnceLock<Option<String>> = OnceLock::new();
    let expected = ADMIN_TOKEN.get_or_init(|| {
        std::fs::read_to_string("/run/choiros/admin.token")
            .ok()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
    });
    match expected {
        Some(expected) => token == expected,
        None => false, // no token file = admin token auth disabled
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_route, strip_path_prefix, RouteTarget};
    use axum::http::StatusCode;

    #[test]
    fn resolve_route_defaults_to_main_pointer() {
        let route = resolve_route("/logs/events").expect("route should resolve");
        match route.target {
            RouteTarget::Pointer(pointer) => assert_eq!(pointer, "main"),
            _ => panic!("expected main pointer route"),
        }
        assert!(route.strip_prefix.is_none());
    }

    #[test]
    fn resolve_route_dev_prefix_pointer() {
        let route = resolve_route("/dev/logs/events").expect("route should resolve");
        match route.target {
            RouteTarget::Pointer(pointer) => assert_eq!(pointer, "dev"),
            _ => panic!("expected dev pointer route"),
        }
        assert_eq!(route.strip_prefix.as_deref(), Some("/dev"));
    }

    #[test]
    fn resolve_route_branch_prefix() {
        let route =
            resolve_route("/branch/feature_login/logs/events").expect("route should resolve");
        match route.target {
            RouteTarget::Branch(branch) => assert_eq!(branch, "feature_login"),
            _ => panic!("expected branch route"),
        }
        assert_eq!(route.strip_prefix.as_deref(), Some("/branch/feature_login"));
    }

    #[test]
    fn resolve_route_rejects_invalid_branch() {
        let resp = resolve_route("/branch/feature%2Flogin/logs").expect_err("route should fail");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn strip_path_prefix_preserves_query() {
        let out = strip_path_prefix("/branch/feat-1/logs/events?limit=1", "/branch/feat-1");
        assert_eq!(out, "/logs/events?limit=1");
    }

    #[test]
    fn strip_path_prefix_normalizes_root() {
        let out = strip_path_prefix("/branch/feat-1", "/branch/feat-1");
        assert_eq!(out, "/");
    }
}
