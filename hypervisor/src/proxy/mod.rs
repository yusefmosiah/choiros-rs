use axum::{
    body::Body,
    extract::{Request, WebSocketUpgrade},
    http::{header, HeaderValue, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use http_body_util::BodyExt;
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;
use tracing::{debug, error};

/// Forward an HTTP request to `target_port`, rewriting the URI.
pub async fn proxy_http(req: Request, target_port: u16) -> Response {
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");

    let target_uri = match Uri::builder()
        .scheme("http")
        .authority(format!("127.0.0.1:{target_port}"))
        .path_and_query(path_and_query)
        .build()
    {
        Ok(u) => u,
        Err(e) => {
            error!("bad proxy URI: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    debug!(%target_uri, "proxying HTTP request");

    let stream = match TcpStream::connect(format!("127.0.0.1:{target_port}")).await {
        Ok(s) => s,
        Err(e) => {
            error!(target_port, "sandbox unreachable: {e}");
            return (StatusCode::BAD_GATEWAY, format!("sandbox unreachable: {e}")).into_response();
        }
    };

    let io = TokioIo::new(stream);
    let (mut sender, conn) = match hyper::client::conn::http1::handshake(io).await {
        Ok(c) => c,
        Err(e) => {
            error!("HTTP/1.1 handshake failed: {e}");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };

    tokio::spawn(async move {
        if let Err(e) = conn.await {
            error!("proxy connection error: {e}");
        }
    });

    let (parts, body) = req.into_parts();
    let mut proxy_req = hyper::Request::from_parts(parts, body);
    *proxy_req.uri_mut() = target_uri;

    // Remove hop-by-hop headers before forwarding.
    proxy_req.headers_mut().remove(header::CONNECTION);
    proxy_req.headers_mut().remove("proxy-connection");
    proxy_req.headers_mut().remove("keep-alive");
    proxy_req.headers_mut().remove(header::TE);
    proxy_req.headers_mut().remove(header::TRAILER);
    proxy_req.headers_mut().remove(header::TRANSFER_ENCODING);
    proxy_req.headers_mut().remove(header::UPGRADE);

    // Fix the Host header so the sandbox sees the correct host
    proxy_req.headers_mut().insert(
        hyper::header::HOST,
        HeaderValue::from_str(&format!("127.0.0.1:{target_port}"))
            .unwrap_or_else(|_| HeaderValue::from_static("localhost")),
    );

    match sender.send_request(proxy_req).await {
        Ok(resp) => {
            let (parts, body) = resp.into_parts();
            let body = Body::new(
                body.map_err(|e| std::io::Error::other(e.to_string()))
                    .boxed_unsync(),
            );
            Response::from_parts(parts, body)
        }
        Err(e) => {
            error!("proxy request failed: {e}");
            StatusCode::BAD_GATEWAY.into_response()
        }
    }
}

/// Forward a WebSocket upgrade to `target_port`.
pub async fn proxy_ws(ws: WebSocketUpgrade, target_port: u16, path: String) -> Response {
    use tokio_tungstenite::connect_async;

    let target_url = format!("ws://127.0.0.1:{target_port}{path}");
    debug!(%target_url, "proxying WebSocket upgrade");

    ws.on_upgrade(move |client_ws| async move {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;

        let (server_ws, _) = match connect_async(&target_url).await {
            Ok(c) => c,
            Err(e) => {
                error!(%target_url, "WS connect to sandbox failed: {e}");
                return;
            }
        };

        let (mut client_sink, mut client_stream) = client_ws.split();
        let (mut server_sink, mut server_stream) = server_ws.split();

        // client → server
        let c2s = async {
            while let Some(msg) = client_stream.next().await {
                match msg {
                    Ok(axum::extract::ws::Message::Text(t)) => {
                        if server_sink
                            .send(Message::Text(t.to_string()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(axum::extract::ws::Message::Binary(b)) => {
                        if server_sink.send(Message::Binary(b.to_vec())).await.is_err() {
                            break;
                        }
                    }
                    Ok(axum::extract::ws::Message::Ping(payload)) => {
                        if server_sink
                            .send(Message::Ping(payload.to_vec()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(axum::extract::ws::Message::Pong(payload)) => {
                        if server_sink
                            .send(Message::Pong(payload.to_vec()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(axum::extract::ws::Message::Close(_)) | Err(_) => break,
                }
            }
        };

        // server → client
        let s2c = async {
            while let Some(msg) = server_stream.next().await {
                match msg {
                    Ok(Message::Text(t)) => {
                        if client_sink
                            .send(axum::extract::ws::Message::Text(t.to_string().into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(Message::Binary(b)) => {
                        if client_sink
                            .send(axum::extract::ws::Message::Binary(b.to_vec().into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(Message::Ping(payload)) => {
                        if client_sink
                            .send(axum::extract::ws::Message::Ping(payload.to_vec().into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(Message::Pong(payload)) => {
                        if client_sink
                            .send(axum::extract::ws::Message::Pong(payload.to_vec().into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
        };

        tokio::select! {
            _ = c2s => {},
            _ = s2c => {},
        }
    })
}

/// Proxy a raw Request that contains a WebSocket upgrade header.
/// Extracts the upgrade and forwards it to the sandbox.
pub async fn proxy_ws_raw(req: Request, target_port: u16, path: String) -> Response {
    use axum::extract::FromRequest;
    match WebSocketUpgrade::from_request(req, &()).await {
        Ok(ws) => proxy_ws(ws, target_port, path).await,
        Err(_) => StatusCode::BAD_REQUEST.into_response(),
    }
}
