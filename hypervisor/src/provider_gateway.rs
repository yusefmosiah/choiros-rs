use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use http_body_util::BodyExt;
use tracing::{error, info, warn};

use crate::{state::ProviderGatewayState, AppState};

#[derive(Debug, Clone, PartialEq, Eq)]
struct GatewayCallerContext {
    sandbox_id: String,
    user_id: String,
    model: String,
}

pub async fn forward_provider_request(
    State(state): State<Arc<AppState>>,
    Path((provider, _rest)): Path<(String, String)>,
    req: Request,
) -> Response {
    let started_at = Instant::now();

    let Some(expected_token) = state.provider_gateway.token.as_deref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "provider gateway not configured",
        )
            .into_response();
    };

    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let provided_token = auth_header
        .strip_prefix("Bearer ")
        .map(str::trim)
        .unwrap_or_default();
    if provided_token != expected_token {
        return (StatusCode::UNAUTHORIZED, "invalid provider gateway token").into_response();
    }

    let context = match caller_context_from_headers(req.headers()) {
        Ok(ctx) => ctx,
        Err((status, msg)) => return (status, msg).into_response(),
    };

    if let Err(response) =
        enforce_per_sandbox_rate_limit(&state.provider_gateway, &context.sandbox_id).await
    {
        return response;
    }

    let upstream_base_url = match req
        .headers()
        .get("x-choiros-upstream-base-url")
        .and_then(|v| v.to_str().ok())
    {
        Some(v) if !v.trim().is_empty() => v.trim(),
        _ => return (StatusCode::BAD_REQUEST, "missing upstream base url").into_response(),
    };

    if !state
        .provider_gateway
        .allowed_upstreams
        .iter()
        .any(|allowed| allowed == upstream_base_url)
    {
        warn!(
            provider = %provider,
            upstream_base_url,
            "blocked provider gateway upstream outside allowlist"
        );
        return (
            StatusCode::FORBIDDEN,
            "upstream not allowed by provider gateway policy",
        )
            .into_response();
    }

    let provider_api_key = match provider_key_for_upstream(upstream_base_url) {
        Ok(v) => v,
        Err((status, msg)) => return (status, msg).into_response(),
    };

    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    let provider_prefix = format!("/provider/v1/{provider}");
    let upstream_path_and_query = path_and_query
        .strip_prefix(&provider_prefix)
        .filter(|s| !s.is_empty())
        .unwrap_or("/");
    let upstream_url = format!(
        "{}{}",
        upstream_base_url.trim_end_matches('/'),
        upstream_path_and_query
    );

    let (parts, body) = req.into_parts();
    let body_bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(e) => {
            error!(error = %e, "failed to read provider gateway request body");
            return (StatusCode::BAD_REQUEST, "invalid request body").into_response();
        }
    };

    let method = reqwest::Method::from_bytes(parts.method.as_str().as_bytes())
        .unwrap_or(reqwest::Method::POST);
    let mut upstream_req = state
        .provider_gateway
        .client
        .request(method, &upstream_url)
        .bearer_auth(provider_api_key)
        .body(body_bytes);
    upstream_req = copy_request_headers(upstream_req, &parts.headers);

    let upstream_res = match upstream_req.send().await {
        Ok(res) => res,
        Err(e) => {
            error!(provider = %provider, upstream_url = %upstream_url, error = %e, "provider gateway upstream request failed");
            return (StatusCode::BAD_GATEWAY, "provider upstream request failed").into_response();
        }
    };

    let status = upstream_res.status();
    let headers = upstream_res.headers().clone();
    let bytes = match upstream_res.bytes().await {
        Ok(b) => b,
        Err(e) => {
            error!(provider = %provider, upstream_url = %upstream_url, error = %e, "provider gateway failed to read upstream response body");
            return (StatusCode::BAD_GATEWAY, "invalid upstream response").into_response();
        }
    };

    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = status;
    copy_response_headers(response.headers_mut(), &headers);

    info!(
        sandbox_id = %context.sandbox_id,
        user_id = %context.user_id,
        provider = %provider,
        model = %context.model,
        status = status.as_u16(),
        latency_ms = started_at.elapsed().as_millis() as u64,
        "provider gateway proxied request"
    );

    response
}

async fn enforce_per_sandbox_rate_limit(
    state: &ProviderGatewayState,
    sandbox_id: &str,
) -> Result<(), Response> {
    let limit = state.rate_limit_per_minute;
    if limit == 0 {
        return Ok(());
    }

    let window = Duration::from_secs(60);
    let now = Instant::now();
    let mut guard = state.rate_limit_state.lock().await;
    let bucket = guard.entry(sandbox_id.to_string()).or_default();

    bucket.retain(|stamp| now.duration_since(*stamp) < window);
    if bucket.len() >= limit {
        warn!(
            sandbox_id = %sandbox_id,
            limit_per_minute = limit,
            "provider gateway rate limit exceeded"
        );
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            "provider gateway rate limit exceeded",
        )
            .into_response());
    }

    bucket.push(now);
    Ok(())
}

fn caller_context_from_headers(
    headers: &HeaderMap,
) -> Result<GatewayCallerContext, (StatusCode, &'static str)> {
    let sandbox_id = headers
        .get("x-choiros-sandbox-id")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or((StatusCode::BAD_REQUEST, "missing sandbox rate-limit key"))?
        .to_string();

    let user_id = headers
        .get("x-choiros-user-id")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("unknown")
        .to_string();

    let model = headers
        .get("x-choiros-model")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("unknown")
        .to_string();

    Ok(GatewayCallerContext {
        sandbox_id,
        user_id,
        model,
    })
}

fn provider_key_for_upstream(
    upstream_base_url: &str,
) -> Result<String, (StatusCode, &'static str)> {
    let key_env = if upstream_base_url.contains("api.z.ai") {
        "ZAI_API_KEY"
    } else if upstream_base_url.contains("api.kimi.com") {
        "KIMI_API_KEY"
    } else if upstream_base_url.contains("api.openai.com") {
        "OPENAI_API_KEY"
    } else {
        return Err((StatusCode::FORBIDDEN, "unsupported provider upstream"));
    };

    std::env::var(key_env).map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "provider api key missing on hypervisor",
        )
    })
}

fn copy_request_headers(
    mut request: reqwest::RequestBuilder,
    headers: &HeaderMap,
) -> reqwest::RequestBuilder {
    for (name, value) in headers {
        if name == header::HOST
            || name == header::CONTENT_LENGTH
            || name == header::AUTHORIZATION
            || name == "x-choiros-upstream-base-url"
            || name == header::CONNECTION
            || name.as_str().eq_ignore_ascii_case("proxy-connection")
            || name.as_str().eq_ignore_ascii_case("keep-alive")
            || name == header::TE
            || name == header::TRAILER
            || name == header::TRANSFER_ENCODING
            || name == header::UPGRADE
        {
            continue;
        }
        request = request.header(name, value);
    }
    request
}

fn copy_response_headers(dest: &mut HeaderMap, src: &HeaderMap) {
    for (name, value) in src {
        if name == header::CONNECTION
            || name.as_str().eq_ignore_ascii_case("proxy-connection")
            || name.as_str().eq_ignore_ascii_case("keep-alive")
            || name == header::TE
            || name == header::TRAILER
            || name == header::TRANSFER_ENCODING
            || name == header::UPGRADE
        {
            continue;
        }
        if let Ok(header_value) = HeaderValue::from_bytes(value.as_bytes()) {
            dest.insert(name, header_value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, sync::Arc};

    use axum::http::HeaderValue;
    use tokio::sync::Mutex;

    #[test]
    fn caller_context_rejects_missing_sandbox_id() {
        let headers = HeaderMap::new();
        let result = caller_context_from_headers(&headers);
        assert_eq!(
            result,
            Err((StatusCode::BAD_REQUEST, "missing sandbox rate-limit key"))
        );
    }

    #[test]
    fn caller_context_defaults_user_and_model() {
        let mut headers = HeaderMap::new();
        headers.insert("x-choiros-sandbox-id", HeaderValue::from_static("u1:live"));

        let result = caller_context_from_headers(&headers).expect("context should parse");
        assert_eq!(result.sandbox_id, "u1:live");
        assert_eq!(result.user_id, "unknown");
        assert_eq!(result.model, "unknown");
    }

    #[tokio::test]
    async fn rate_limit_blocks_after_budget() {
        let state = ProviderGatewayState {
            token: None,
            base_url: None,
            allowed_upstreams: Vec::new(),
            client: reqwest::Client::new(),
            rate_limit_per_minute: 2,
            rate_limit_state: Arc::new(Mutex::new(HashMap::new())),
        };

        assert!(enforce_per_sandbox_rate_limit(&state, "u1:live")
            .await
            .is_ok());
        assert!(enforce_per_sandbox_rate_limit(&state, "u1:live")
            .await
            .is_ok());

        let third = enforce_per_sandbox_rate_limit(&state, "u1:live").await;
        assert!(third.is_err());
        let response = third.err().expect("third call should be rate limited");
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}
