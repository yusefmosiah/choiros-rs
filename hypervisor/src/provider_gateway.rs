use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use http_body_util::BodyExt;
use tracing::{error, warn};

use crate::AppState;

pub async fn forward_provider_request(
    State(state): State<Arc<AppState>>,
    Path((provider, _rest)): Path<(String, String)>,
    req: Request,
) -> Response {
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
    response
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
