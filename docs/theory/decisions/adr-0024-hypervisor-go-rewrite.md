# ADR-0024: Hypervisor Go Rewrite

Date: 2026-03-11
Kind: Decision
Status: Proposed
Priority: 2
Requires: [ADR-0007, ADR-0014]
Authors: wiz + Claude

## Narrative Summary (1-minute read)

The hypervisor is a ~6.5k LOC Rust control-plane microservice that
authenticates users, manages VM lifecycle, proxies traffic to sandboxes, and
gateways LLM API requests. It is not an application framework or agent
orchestrator — it is HTTP handlers, process management, and proxying.

This ADR rewrites the hypervisor in Go as the first bounded experiment from
the Go feasibility study. The hypervisor is the right target because:

- it has a clean external boundary (HTTP API, systemd, cloud-hypervisor CLI)
- it can be tested end-to-end against the existing Playwright suite
- it can be swapped transparently without touching the sandbox runtime
- it exercises the full deploy pipeline (nix build, systemd, Caddy)

The sandbox runtime is explicitly out of scope. It stays in Rust. If the
hypervisor rewrite succeeds and the operational experience is positive, the
sandbox can be approached later as a protocol-boundary exercise per the Go
feasibility report's recommendation.

## What Changed

- 2026-03-11: Initial ADR from Go feasibility study recommendations.

## What To Do Next

Run the implementation guide (`docs/theory/guides/adr-0024-implementation.md`)
as an autonomous agent task. The guide is written as a self-driving prompt.

---

## Context

The Go refactor feasibility study (`docs/state/reports/go-refactor-feasibility-2026-03-09.md`)
recommended:

1. Do not rewrite solely for build times.
2. Use `hypervisor` as the first bounded Go experiment.
3. Treat `sandbox` as a protocol before treating it as a replacement target.

The hypervisor is the right first target because it is a stateless-ish
control plane with well-defined external interfaces:

- 28 HTTP endpoints (auth, admin, provider gateway, proxy)
- SQLite database (8 tables, simple CRUD)
- Cloud-hypervisor process lifecycle (spawn, stop, hibernate via CLI)
- systemd unit management (optional, ADR-0017)
- WebAuthn passkey authentication
- HTTP/WebSocket reverse proxy to sandbox VMs

There is no actor system, no event sourcing, no LLM orchestration — those
live in the sandbox. The hypervisor is "just" a well-structured web service.

---

## Decision 1: Rewrite the Hypervisor in Go

Rewrite all hypervisor functionality in Go with behavioral parity as the
success criterion. Same HTTP API, same database schema, same external
command invocations, same proxy behavior.

### What stays the same

- All 28 HTTP endpoints with identical routes and response shapes
- SQLite database schema (8 tables, same migrations)
- WebAuthn passkey registration and login flow
- Provider gateway request forwarding and rate limiting
- HTTP and WebSocket reverse proxy to sandbox VMs
- VM lifecycle management via runtime-ctl binary
- systemd unit integration (ADR-0017)
- Idle watchdog behavior (hibernate after timeout)
- Memory pressure monitoring
- Session storage (SQLite-backed)

### What changes

- Language: Rust -> Go
- Web framework: axum -> net/http (stdlib) or chi
- Database: sqlx -> database/sql + go-sqlite3 or modernc.org/sqlite
- WebAuthn: webauthn-rs -> go-webauthn/webauthn
- HTTP proxy: hyper -> net/http/httputil.ReverseProxy
- WebSocket proxy: tokio-tungstenite -> gorilla/websocket or nhooyr.io/websocket
- Session: tower-sessions -> gorilla/sessions or custom SQLite store
- Build: cargo/crane -> go build (nix buildGoModule)

### Consequences

- Single static binary, fast compilation, simple cross-compilation
- Standard library covers most HTTP, proxy, and process management needs
- Go WebAuthn libraries are mature (go-webauthn is well-maintained)
- SQLite via CGo (go-sqlite3) or pure Go (modernc.org/sqlite)
- Nix packaging via `buildGoModule` is straightforward

---

## Decision 2: Behavioral Parity via E2E Testing

The rewrite is correct when the existing Playwright E2E suite passes against
the Go binary with no test modifications. The Go hypervisor must be a
transparent drop-in replacement.

Testing strategy:

1. Run Playwright E2E tests against Go binary on the same port (9090)
2. Run the capacity stress test (ADR-0022) against Go binary
3. Compare: auth flow, sandbox boot, proxy behavior, provider gateway,
   WebSocket streaming, idle hibernation

The agent prompt in the implementation guide is designed to iterate until
all E2E tests pass.

### Consequences

- No need to write new Go-specific tests for behavioral parity
- E2E suite is the source of truth for "does it work the same"
- Performance comparison comes from stress test results
- Unit tests in Go are welcome but not the acceptance criterion

---

## Decision 3: Nix-Native Build and Deploy

The Go binary must be buildable via nix flake and deployable through the
existing CI pipeline. It should be a drop-in replacement in the systemd
service definition.

```nix
# hypervisor-go/flake.nix or root flake.nix
hypervisor-go = pkgs.buildGoModule {
  pname = "hypervisor";
  version = "0.1.0";
  src = ./hypervisor-go;
  vendorHash = "...";
};
```

The deploy flow remains: push to main -> CI builds on server -> systemd
restart. Only the build command changes (nix builds Go instead of Rust).

### Consequences

- Same deploy pipeline, different build backend
- Can A/B test by deploying Rust to Node A and Go to Node B (or vice versa)
- Rollback is trivial: point systemd at the old Rust binary

---

## Decision 4: Sandbox Stays in Rust

The sandbox runtime is explicitly not part of this rewrite. It stays in Rust.
The hypervisor communicates with sandboxes via:

- HTTP/WebSocket proxy (network boundary)
- runtime-ctl binary invocation (process boundary)
- systemd unit management (OS boundary)

These are all protocol boundaries. The sandbox does not need to know or care
what language the hypervisor is written in.

### Consequences

- Blast radius is limited to the control plane
- No risk to the actor system, event sourcing, or LLM orchestration
- If the Go rewrite fails or regresses, rollback is instant

---

## Non-Goals

- Rewriting the sandbox runtime
- Changing the HTTP API contract
- Changing the database schema
- Adding new features during the rewrite
- Optimizing beyond behavioral parity (that comes after)

---

## Source References

- Go feasibility study: `docs/state/reports/go-refactor-feasibility-2026-03-09.md`
- Hypervisor source: `hypervisor/src/`
- E2E tests: `tests/playwright/`
- Capacity stress test: `tests/playwright/capacity-stress-test.spec.ts`
- Systemd service: `nix/hosts/ovh-node.nix`
- ADR-0022 (concurrency optimizations): apply to Go version too
