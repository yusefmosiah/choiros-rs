use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use tower_sessions::Session;

use crate::{auth::session as sess, sandbox::SandboxRole, AppState};

/// Middleware: require an authenticated session.
/// Unauthenticated requests to non-auth paths are redirected to /login.
pub async fn require_auth(
    State(_state): State<Arc<AppState>>,
    session: Session,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path();

    // Allow auth endpoints, auth pages, and static frontend assets through unauthenticated
    if path.starts_with("/auth/")
        || path.starts_with("/assets/")
        || path.starts_with("/wasm/")
        || path.starts_with("/provider/v1/")
        || path == "/"
        || path == "/login"
        || path == "/register"
        || path == "/recovery"
    {
        return next.run(req).await;
    }

    if sess::get_user_id(&session).await.is_none() {
        return Redirect::to("/login").into_response();
    }

    next.run(req).await
}

/// Fallback handler: proxy authenticated traffic to the appropriate sandbox.
/// Requests to `/dev/...` go to the dev sandbox; everything else to live.
pub async fn proxy_to_sandbox(
    State(state): State<Arc<AppState>>,
    session: Session,
    req: Request,
) -> Response {
    let user_id = match sess::get_user_id(&session).await {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, "not authenticated").into_response(),
    };

    let path = req.uri().path().to_string();
    let (role, effective_path) = if path.starts_with("/dev/") {
        (
            SandboxRole::Dev,
            path.trim_start_matches("/dev").to_string(),
        )
    } else {
        (SandboxRole::Live, path.clone())
    };

    // Ensure the sandbox is running (auto-starts if needed)
    let port = match state.sandbox_registry.ensure_running(&user_id, role).await {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("sandbox unavailable: {e}"),
            )
                .into_response();
        }
    };

    // Detect WebSocket upgrade
    let is_ws = req
        .headers()
        .get("upgrade")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);

    let req = sanitize_and_tag_proxy_request(req, &user_id, role);

    if is_ws {
        let path_with_query = req
            .uri()
            .path_and_query()
            .map(|pq| pq.as_str().to_string())
            .unwrap_or(effective_path.clone());
        let path_to_proxy = if role == SandboxRole::Dev {
            // Strip the /dev prefix for the sandbox
            path_with_query
                .strip_prefix("/dev")
                .unwrap_or(&path_with_query)
                .to_string()
        } else {
            path_with_query
        };
        return crate::proxy::proxy_ws_raw(req, port, path_to_proxy).await;
    }

    // For dev sandbox, rewrite the path to strip the /dev prefix
    if role == SandboxRole::Dev && path.starts_with("/dev/") {
        let (mut parts, body) = req.into_parts();
        let new_path = path.trim_start_matches("/dev").to_string();
        let new_path_and_query = if let Some(q) = parts.uri.query() {
            format!("{new_path}?{q}")
        } else {
            new_path
        };
        if let Ok(uri) = new_path_and_query.parse::<axum::http::Uri>() {
            parts.uri = uri;
        }
        let req = Request::from_parts(parts, body);
        return crate::proxy::proxy_http(req, port).await;
    }

    crate::proxy::proxy_http(req, port).await
}

fn sanitize_and_tag_proxy_request(req: Request, user_id: &str, role: SandboxRole) -> Request {
    let (mut parts, body) = req.into_parts();

    // Never forward browser session credentials or client auth headers into sandbox.
    parts.headers.remove(header::COOKIE);
    parts.headers.remove(header::AUTHORIZATION);
    parts.headers.remove(header::PROXY_AUTHORIZATION);

    if let Ok(v) = HeaderValue::from_str(user_id) {
        parts.headers.insert("x-choiros-user-id", v);
    }
    let role_value = match role {
        SandboxRole::Live => "live",
        SandboxRole::Dev => "dev",
    };
    parts.headers.insert(
        "x-choiros-sandbox-role",
        HeaderValue::from_static(role_value),
    );
    parts.headers.insert(
        "x-choiros-proxy-authenticated",
        HeaderValue::from_static("true"),
    );

    Request::from_parts(parts, body)
}
