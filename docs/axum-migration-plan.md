# Actix-Web -> Axum/Tower Migration Plan (ChoirOS Sandbox)

Date: 2026-02-04
Status: Completed (2026-02-05)

Note: The Actix inventory below is historical context.

## Goal
Replace `actix-web` (and related Actix web stack crates) with `axum` + `tower` in the Sandbox HTTP/WebSocket server, while keeping the existing actor system (ractor) and API behavior intact. This plan focuses on the current code footprint and concrete changes required.

## Previous Actix Usage (Historical Inventory)

Primary server:
- `sandbox/src/main.rs` — `HttpServer`, `App`, `web::Data`, `actix-cors`, routes `/health`, `/ws`, and `api::config`.

HTTP routes:
- `sandbox/src/api/mod.rs` — configures all routes via `web::ServiceConfig`.
- `sandbox/src/api/chat.rs` — `#[post]`, `#[get]` macros, uses `web::Json`, `web::Path`, returns `HttpResponse`.
- `sandbox/src/api/desktop.rs` — same pattern as chat.

WebSocket routes (two implementations):
- `sandbox/src/api/websocket.rs` — uses `actix-ws` with `actix_ws::Session` and `actix_ws::Message`.
- `sandbox/src/api/websocket_chat.rs` — uses `actix-web-actors` and `actix::Actor` to handle chat WS.

Tests:
- `sandbox/tests/chat_api_test.rs`, `sandbox/tests/desktop_api_test.rs`, `sandbox/tests/websocket_chat_test.rs` — use Actix test harness (`actix_web::test`, `actix_test`, `#[actix_web::test]`), plus `actix_http::ws` for WS tests.

Dependencies:
- `sandbox/Cargo.toml` uses `actix`, `actix-web`, `actix-rt`, `actix-ws`, `actix-cors`, `actix-web-actors`, and dev deps `actix-http`, `actix-service`, `actix-test`.
- `hypervisor/Cargo.toml` previously listed Actix deps but `hypervisor/src/main.rs` is a placeholder.

Documentation references:
- `README.md`, `docs/ARCHITECTURE_SPECIFICATION.md`, `docs/TESTING_STRATEGY.md`, and other docs referenced Actix as the backend stack.

## Key Behavior to Preserve
- REST endpoints and JSON payloads:
  - `/health`
  - `/chat/send` (POST)
  - `/chat/{actor_id}/messages` (GET)
  - Desktop endpoints under `/desktop/{desktop_id}`
- WebSocket endpoints:
  - `/ws` (desktop events, subscribe/ping/pong)
  - `/ws/chat/{actor_id}` and `/ws/chat/{actor_id}/{user_id}` (chat streaming)
- CORS behavior and allowed origins in `sandbox/src/main.rs`.

## Axum/Tower Design Decisions (Needed Early)
These are choices that materially shape the code, especially with imminent WebSockets + auth:
1. State model: use `axum::extract::State<AppState>` with `AppState` stored in `Arc` (recommended) vs cloning `AppState` directly.
2. Error model: return `axum::Json` + `StatusCode` or define a unified error type implementing `IntoResponse`.
3. Auth strategy (now, not later): bearer tokens vs cookie sessions; required to inform CORS, WS auth, and UI integration.
4. WS session registry: store `mpsc::UnboundedSender<Message>` per client, or use `broadcast::Sender` per desktop.
5. Middleware stack: use `tower_http` layers for CORS, trace/logging, request IDs, compression.
6. WS auth path: cookie-based auth (automatic in browser WS) vs query/subprotocol tokens.

## Migration Plan (Concrete Steps)

### Phase 1: Foundation (Dependencies + Router)
1. Add Axum/Tower dependencies to `sandbox/Cargo.toml`:
   - `axum`, `tower`, `tower-http`, `hyper`, `http`.
   - Remove `actix-*` crates from runtime deps once compiled.
2. Replace `HttpServer` bootstrap in `sandbox/src/main.rs` with Axum:
   - Build `Router` from `api::router()`.
   - Configure CORS with `tower_http::cors::CorsLayer`.
   - `axum::serve` with `tokio::net::TcpListener`.
3. Convert `sandbox/src/api/mod.rs` to return a `Router` instead of using `ServiceConfig`.

### Phase 2: Auth Skeleton + HTTP Handlers
4. Introduce a minimal auth extractor early (before full port):
   - Define `CurrentUser` (or `AuthContext`) extracted from headers/cookies.
   - For now, allow a “dev mode” fallback (e.g., anonymous user) behind a feature flag.
   - Wire it as an Axum extractor (`FromRequestParts`) so REST + WS can share it.
5. Migrate `sandbox/src/api/chat.rs`:
   - Replace `#[post]`/`#[get]` macros with plain async fns.
   - Replace `web::Json`, `web::Path`, `web::Data` with Axum extractors.
   - Return `(StatusCode, Json<T>)` or `Json<T>`.
6. Migrate `sandbox/src/api/desktop.rs` similarly.
7. Ensure route wiring in `api::router()` matches current paths.

### Phase 3: WebSockets
8. Replace `sandbox/src/api/websocket.rs` (desktop WS) with Axum WebSocket handler:
   - Use `WebSocketUpgrade` and `on_upgrade`.
   - Split socket into sender/receiver (`socket.split()`), maintain session registry with sender channels.
   - Convert `actix_ws::Message` to `axum::extract::ws::Message`.
   - Enforce auth at WS upgrade time; reject if unauthenticated.
9. Replace `sandbox/src/api/websocket_chat.rs` (chat WS):
   - Remove `actix-web-actors` and `actix::Actor` usage.
   - Implement a single async WS loop that:
     - Parses incoming `ClientMessage` JSON.
     - Uses `ractor::call!` for `ChatAgentMsg` as before.
     - Streams chunked responses by sending text frames.
   - Reuse the auth extractor (or copy auth parsing for the upgrade path).

### Phase 4: Tests
10. Replace Actix test harness with Axum + Tower testing:
   - Use `tower::ServiceExt::oneshot` on `Router`.
   - Replace `#[actix_web::test]` with `#[tokio::test]`.
   - Update WS tests to use `tokio-tungstenite` or `hyper` ws client.
11. Remove `actix-http`, `actix-test`, and `actix-service` from dev deps.

### Phase 5: Cleanup and Docs
12. Remove `sandbox/src/api/main.rs` if unused (or explicitly document it as legacy).
13. Remove Actix deps from `hypervisor/Cargo.toml` unless a near-term plan requires them.
14. Update docs that state “Actix Web” backend:
   - `README.md`
   - `docs/ARCHITECTURE_SPECIFICATION.md`
   - `docs/TESTING_STRATEGY.md`
   - Other architecture docs referencing Actix.

## Detailed Mapping (Actix -> Axum)

Core types:
- `actix_web::web::Data<T>` -> `axum::extract::State<T>` or `State<Arc<T>>`
- `actix_web::web::Json<T>` -> `axum::Json<T>`
- `actix_web::web::Path<T>` -> `axum::extract::Path<T>`
- `actix_web::HttpResponse` -> `(StatusCode, Json<T>)` or `Json<T>`

CORS:
- `actix_cors::Cors` -> `tower_http::cors::CorsLayer`

WS:
- `actix_ws::handle` -> `WebSocketUpgrade::on_upgrade`
- `actix_ws::Session` -> store an `mpsc::UnboundedSender<Message>` per connection
- `actix_web_actors::ws` -> manual WS loop (Tokio task)

Tests:
- `actix_web::test::init_service` -> `Router::oneshot` via `tower::ServiceExt`
- `#[actix_web::test]` -> `#[tokio::test]`

## Known Migration Hotspots
- WebSocket chat currently uses Actix Actor integration. This is the largest refactor due to lifetime + actor context differences.
- The desktop WS session map uses `actix_ws::Session` (cloneable). Axum sockets are not cloneable; this requires redesign around sender channels or broadcast.
- Extractor ordering in Axum (only one body-consuming extractor; it must be last) requires careful signature ordering.

## Auth + WebSocket Readiness Notes
- Browser WebSockets cannot set arbitrary headers; auth options are:
  - Cookie-based session (recommended for browser WS if auth is soon).
  - Token passed via query param or `Sec-WebSocket-Protocol`.
- If using cookies, CORS must enable credentials and origins must be explicit (no wildcard).
- For REST, a shared extractor should validate auth once, then attach `CurrentUser` to handlers.
- For WS, validate auth during upgrade and embed `CurrentUser` in the connection task.
- Plan for per-desktop authorization checks when handling `subscribe` messages.

## Proposed WS Session Design (Desktop)
Recommendation:
- Maintain `HashMap<String, Vec<mpsc::UnboundedSender<Message>>>>` keyed by `desktop_id`.
- For each WS connection, create a `mpsc::unbounded_channel` for outbound messages, and drive a sender task that forwards channel messages to the socket.
- On subscribe, store sender in the registry.

This is a near-direct substitute for Actix’s `Session` clone storage. With auth, store `(desktop_id, user_id, sender)` so broadcasts can be filtered.

## Milestones and Acceptance Criteria

Milestone A: Axum server boots and `/health` responds.
- Build and run `just dev-sandbox`.
- `GET /health` returns JSON with same shape.

Milestone B: Auth extractor wired into REST (even if permissive), and Chat + Desktop REST endpoints behave as before.
- All tests in `sandbox/tests/chat_api_test.rs` and `sandbox/tests/desktop_api_test.rs` pass after conversion.

Milestone C: Desktop WebSocket works.
- `sandbox-ui` desktop connects to `/ws` and receives `desktop_state` and updates.

Milestone D: Chat WebSocket works (with auth).
- `sandbox/tests/websocket_chat_test.rs` ported and passing (or replaced with a new WS test).

Milestone E: Actix dependencies removed.
- `rg actix` should only match docs or archived references, not code or Cargo deps.

## Estimated Effort (Rough)
- Phase 1-2 (REST + server): 0.5–1 day
- Phase 3 (WS): 1–2 days
- Phase 4 (tests): 0.5–1 day
- Phase 5 (docs + cleanup): 0.5 day

## Open Questions
1. Auth choice: cookie sessions or bearer tokens? This impacts CORS and WS auth.
2. WS auth transport: cookie, query param, or `Sec-WebSocket-Protocol`?
3. Should we introduce a unified error type now, or preserve the current JSON error shapes as-is?
4. For WebSockets, do we need backpressure handling or a bounded channel for outbound messages?
5. Are any external clients relying on Actix-specific behavior (e.g., header order, error format)?

## Appendix: Files to Touch
- Server and API:
  - `sandbox/src/main.rs`
  - `sandbox/src/api/mod.rs`
  - `sandbox/src/api/chat.rs`
  - `sandbox/src/api/desktop.rs`
  - `sandbox/src/api/websocket.rs`
  - `sandbox/src/api/websocket_chat.rs`
- Tests:
  - `sandbox/tests/chat_api_test.rs`
  - `sandbox/tests/desktop_api_test.rs`
  - `sandbox/tests/websocket_chat_test.rs`
- Dependencies:
  - `sandbox/Cargo.toml`
  - `hypervisor/Cargo.toml`
- Docs:
  - `README.md`
  - `docs/ARCHITECTURE_SPECIFICATION.md`
  - `docs/TESTING_STRATEGY.md`
