# Implementing ADR-0024: Hypervisor Go Rewrite

Date: 2026-03-11
Kind: Guide
Status: Active
Priority: 2
Requires: [ADR-0024]

## Narrative Summary (1-minute read)

This document is a self-driving prompt for an autonomous coding agent. It
rewrites the ChoirOS hypervisor from Rust to Go with full behavioral parity.
The agent should work through the phases in order, running E2E tests after
each phase, and iterating until all tests pass. The rewrite is complete when
the Go binary is a transparent drop-in for the Rust binary.

## What Changed

- 2026-03-11: Initial implementation guide.

## What To Do Next

Give this document to a coding agent with access to the repository and a
Linux environment (or the target deployment node). The agent should be able
to run `go build`, `nix build`, and Playwright tests.

---

## Agent Prompt

You are rewriting the ChoirOS hypervisor from Rust to Go. The hypervisor is a
~6.5k LOC control-plane microservice. Your job is to produce a Go binary that
is a transparent drop-in replacement for the Rust binary — same HTTP API, same
database schema, same external behavior.

### Success criterion

The existing Playwright E2E test suite passes against your Go binary with zero
test modifications. You are done when all tests pass. Until then, keep working.

### Source of truth

The Rust source is in `hypervisor/src/`. Read every file before you start
writing Go. The key files and their responsibilities:

| File | LOC | What it does |
|------|-----|-------------|
| `main.rs` | 177 | App init, route setup, server start |
| `state.rs` | 26 | AppState struct (db, webauthn, registry, gateway) |
| `config.rs` | 171 | Environment variable parsing |
| `db/mod.rs` | 64 | SQLite connection + migrations |
| `session_store.rs` | 160 | SQLite-backed session storage |
| `auth/mod.rs` | 79 | WebAuthn builder, recovery code gen |
| `auth/handlers.rs` | 909 | Register/login/logout/recovery endpoints |
| `auth/session.rs` | 30 | Session utilities |
| `middleware.rs` | 491 | Auth enforcement, bootstrap serving, routing |
| `api/mod.rs` | 243 | Admin sandbox management endpoints |
| `sandbox/mod.rs` | 1107 | VM registry, lifecycle, idle watchdog |
| `sandbox/systemd.rs` | 572 | systemd unit templating |
| `proxy/mod.rs` | 231 | HTTP + WebSocket reverse proxy |
| `provider_gateway.rs` | 730 | LLM API forwarding, rate limiting |
| `runtime_registry.rs` | 341 | Database ops for runtime/route pointers |

Also read:
- `hypervisor/Cargo.toml` for dependency versions
- `hypervisor/migrations/` for the exact database schema
- `nix/hosts/ovh-node.nix` for systemd service config and environment
- `tests/playwright/` for E2E test expectations

### Output location

Write the Go code in `hypervisor-go/` at the repository root. Structure:

```
hypervisor-go/
├── go.mod
├── go.sum
├── main.go
├── config.go
├── state.go
├── db.go
├── session.go
├── auth.go
├── middleware.go
├── admin.go
├── sandbox.go
├── systemd.go
├── proxy.go
├── provider_gateway.go
├── runtime_registry.go
└── migrations/
    ├── 0001_initial.sql
    └── 0002_runtime_registry.sql
```

Keep it flat. Do not create packages until complexity demands it. Copy the
SQL migration files from `hypervisor/migrations/` verbatim.

### Phase 1: Skeleton and config

1. Initialize `go.mod` with module path `github.com/nicholasgasior/choiros/hypervisor-go`
   (or whatever the repo module path is — check the repo).
2. Parse all environment variables from `config.rs`. Use the same env var
   names, same defaults.
3. Set up SQLite database connection. Apply migrations on startup (embed them).
4. Create the AppState struct equivalent.
5. Start an HTTP server on the configured port.
6. Verify: `curl http://localhost:9090/` should return something (even 404).

### Phase 2: Database and sessions

1. Port the session store (SQLite-backed). Match the `sessions` table schema.
2. Port the database operations from `runtime_registry.rs`.
3. Verify: server starts, creates database, applies migrations.

### Phase 3: Authentication

This is the most complex module. WebAuthn passkey auth requires precise
protocol compliance.

1. Use `github.com/go-webauthn/webauthn` for WebAuthn.
2. Port all auth handlers: register start/finish, login start/finish,
   logout, recovery, /auth/me.
3. Store passkeys as JSON blobs (same as Rust version).
4. Hash recovery codes with argon2 (same parameters as Rust version —
   read the Rust source for exact argon2 config).
5. Audit log writes on auth events.
6. Session cookie behavior must match: name, path, SameSite, HttpOnly,
   max-age. Read `main.rs` for the session layer config.

**Critical detail**: WebAuthn challenge state is stored in the session during
the start phase and consumed in the finish phase. If session handling is
wrong, WebAuthn will always fail. Test this early.

Verify: Run the Playwright auth tests. Registration and login must work
with virtual authenticators.

### Phase 4: Middleware

1. Port the auth middleware. The Rust middleware checks the session for a
   user ID and either passes through (authenticated) or redirects to login.
2. Port the bootstrap asset serving (serves the Dioxus WASM frontend).
   Read `middleware.rs` carefully — it serves static files from
   `FRONTEND_DIST` for unauthenticated paths and proxies everything else.
3. The middleware routing logic determines whether a request goes to:
   - auth endpoints (no auth required)
   - admin endpoints (auth required)
   - provider gateway (auth required, special routing)
   - sandbox proxy (auth required, default fallback)

Verify: Unauthenticated requests get the login page. Authenticated requests
reach sandbox proxy.

### Phase 5: Sandbox registry and VM lifecycle

1. Port `SandboxRegistry` — the in-memory map of user sandboxes.
2. Port `ensure_running()` — the core lifecycle function that boots VMs
   on demand. This calls the runtime-ctl binary via `exec.Command`.
3. Port the idle watchdog — background goroutine that hibernates inactive
   VMs after timeout.
4. Port memory pressure checking (`/proc/meminfo` parsing).
5. Port `allocate_port()` — finding an available port for new VMs.

**Critical detail**: `ensure_running()` is called on every proxied request.
It must be fast for the common case (VM already running). Read the Rust
source carefully for the lock acquisition pattern and status transitions.

**Critical detail**: The runtime-ctl binary is called with specific arguments
and environment variables. Read `sandbox/mod.rs:run_runtime_ctl` for the
exact invocation. Stdout/stderr are redirected to `/dev/null`.

Verify: Boot a sandbox VM through the Go hypervisor. Check that
`cloud-hypervisor` processes appear.

### Phase 6: HTTP and WebSocket proxy

1. Use `net/http/httputil.ReverseProxy` for HTTP proxying.
2. For WebSocket, detect the `Upgrade: websocket` header and do a
   bidirectional proxy (read the Rust `proxy/mod.rs` for the exact logic).
3. The proxy target is `127.0.0.1:{port}` where port comes from the
   sandbox registry.

**Critical detail**: WebSocket proxying must be bidirectional and handle
close frames correctly. The sandbox uses WebSockets for real-time streaming
(actor calls, terminal output, desktop events).

Verify: Open the app in a browser through the Go proxy. WebSocket
connections should work (check browser devtools for WS frames).

### Phase 7: Provider gateway

1. Port the LLM API forwarding logic. The gateway receives requests at
   `/provider/v1/{provider}/{rest...}` and forwards them to upstream APIs.
2. Port the Bedrock request rewriting (Anthropic Messages API format ->
   Bedrock InvokeModel format). Read `provider_gateway.rs` carefully for
   the URL rewriting, header manipulation, and body transformation.
3. Port the per-sandbox rate limiter (rolling window, requests per minute).
4. Port the auth token injection (bearer token for upstream APIs).

**Critical detail**: The provider gateway handles streaming responses
(SSE). The response body must be streamed through, not buffered. Use
`io.Copy` or equivalent streaming pattern.

Verify: From inside a sandbox, make an LLM API call through the gateway.
Streaming responses should work.

### Phase 8: Admin endpoints

1. Port all `/admin/` endpoints from `api/mod.rs`.
2. These are straightforward CRUD operations on the sandbox registry.

Verify: `curl /admin/sandboxes` returns the correct VM state.

### Phase 9: systemd integration

1. Port `sandbox/systemd.rs` — the systemd unit templating for VM lifecycle.
2. This is used when `CHOIR_SYSTEMD_LIFECYCLE=1` is set.
3. Port the unit file generation and `systemctl` invocations.

Verify: With systemd lifecycle enabled, VMs start via systemd units.

### Phase 10: Full E2E verification

Run the full Playwright E2E suite against the Go binary:

```bash
cd tests/playwright
PLAYWRIGHT_SANDBOX_BASE_URL=http://localhost:9090 \
  npx playwright test --project=sandbox
```

If tests fail:
1. Read the failure output carefully.
2. Compare the Go behavior to the Rust behavior for that specific endpoint.
3. Fix the Go code.
4. Re-run the failing test.
5. Repeat until all tests pass.

Do not modify the Playwright tests. The tests define correct behavior.
If a test fails, your Go code is wrong.

### Phase 11: Nix build

Create a nix flake output for the Go binary:

```nix
# In hypervisor-go/flake.nix or add to root flake.nix
hypervisor-go = pkgs.buildGoModule {
  pname = "hypervisor";
  version = "0.1.0";
  src = ./hypervisor-go;
  vendorHash = "sha256-XXXX"; # nix will tell you the correct hash
  CGO_ENABLED = 1; # if using go-sqlite3
  buildInputs = [ pkgs.sqlite ];
};
```

Verify: `nix build .#hypervisor-go` produces a working binary.

### Iteration protocol

After each phase:
1. Build and start the Go binary.
2. Run the relevant subset of E2E tests.
3. If tests fail, debug and fix before moving to the next phase.
4. If you are stuck on a specific behavior, read the Rust source again.
   The Rust code is the specification.

After all phases:
1. Run the full E2E suite.
2. Run the capacity stress test if available on the target node.
3. Document any behavioral differences you discovered.

### Go library recommendations

These are suggestions, not requirements. Use whatever works.

| Concern | Recommendation | Why |
|---------|---------------|-----|
| HTTP router | `net/http` stdlib or `chi` | Simple, no framework overhead |
| SQLite | `modernc.org/sqlite` (pure Go) or `github.com/mattn/go-sqlite3` (CGo) | Pure Go avoids CGo complexity but CGo is faster |
| WebAuthn | `github.com/go-webauthn/webauthn` | Most maintained Go WebAuthn library |
| WebSocket | `nhooyr.io/websocket` or `github.com/gorilla/websocket` | nhooyr is more modern, gorilla is battle-tested |
| Sessions | Custom SQLite store | Simple, matches existing schema |
| Argon2 | `golang.org/x/crypto/argon2` | stdlib-adjacent, correct implementation |
| Logging | `log/slog` | stdlib structured logging, matches tracing output |
| UUID | `github.com/google/uuid` | Standard |
| Migration | Embed SQL files with `embed` package | Simple, no migration framework needed |

### What NOT to do

- Do not add features. Parity only.
- Do not change the HTTP API. Same routes, same response shapes.
- Do not change the database schema. Same tables, same columns.
- Do not optimize prematurely. Get it working first.
- Do not create a complex package structure. Flat is fine for 6.5k LOC.
- Do not modify the Playwright tests.
- Do not rewrite the sandbox. It stays in Rust.

### Environment variables (complete list)

Copy these from `hypervisor/src/config.rs`. The Go binary must read the same
env vars with the same defaults:

```
PORT                          (default: 9090)
DATABASE_URL                  (default: sqlite:./data/hypervisor.db)
WEBAUTHN_RP_ID               (required)
WEBAUTHN_RP_ORIGIN           (required)
WEBAUTHN_RP_NAME             (default: "ChoirOS")
SANDBOX_RUNTIME_CTL          (default: "vfkit-runtime-ctl")
SANDBOX_LIVE_PORT            (default: 8080)
SANDBOX_DEV_PORT             (default: 8081)
SANDBOX_BRANCH_PORT_START    (default: 12000)
SANDBOX_BRANCH_PORT_END      (default: 12999)
SANDBOX_IDLE_TIMEOUT         (default: 1800, in seconds)
FRONTEND_DIST                (default: "../dioxus-desktop/dist")
PROVIDER_GATEWAY_TOKEN       (optional)
PROVIDER_GATEWAY_BASE_URL    (optional)
PROVIDER_GATEWAY_ALLOWED_UPSTREAMS (optional, comma-separated)
PROVIDER_GATEWAY_RATE_LIMIT  (default: 60)
CHOIR_SYSTEMD_LIFECYCLE      (optional, "1" to enable)
CHOIR_SANDBOX_ROOT           (passed to runtime-ctl)
CHOIR_SANDBOX_BINARY         (passed to runtime-ctl)
CHOIR_SANDBOX_DATABASE_URL   (passed to runtime-ctl)
CHOIR_PROVIDER_GATEWAY_URL   (passed to runtime-ctl)
CHOIR_PROVIDER_GATEWAY_TOKEN (passed to runtime-ctl)
```

### Database schema

Copy the migration files verbatim from `hypervisor/migrations/`. Apply them
on startup using Go's `embed` package and raw SQL execution. The schema is:

**0001_initial.sql**: users, passkeys, recovery_codes, audit_log, sessions
**0002_runtime_registry.sql**: user_vms, branch_runtimes, route_pointers, runtime_events

Do not use an ORM. Use `database/sql` with raw queries. The Rust code uses
raw SQL via sqlx — match that pattern.

---

## Estimated Effort

An autonomous coding agent with access to the Rust source and E2E tests
should complete this in 8-16 hours of continuous work. The hypervisor is
straightforward HTTP handlers and process management — no exotic concurrency,
no distributed systems, no complex state machines.

The hardest parts will be:
1. WebAuthn protocol compliance (Phase 3) — subtle session state management
2. WebSocket proxy (Phase 6) — bidirectional streaming with correct close
3. Provider gateway streaming (Phase 7) — SSE pass-through without buffering

Everything else is mechanical translation.
