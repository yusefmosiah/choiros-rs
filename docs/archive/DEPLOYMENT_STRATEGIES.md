# Deployment Strategies (Current + Future Options)

This document summarizes how ChoirOS is deployed today and outlines upgrade paths. It is intentionally practical and pre‑MVP oriented.

---

## Current Deployment (Pre‑MVP, Fast Iteration)

**Goal:** Ship quickly with minimal moving parts.

**Architecture**
- Single EC2 instance (Ubuntu 22.04)
- Systemd services for backend + frontend
- Caddy reverse proxy (HTTP only on bare IP)
- SQLite/libsql on local disk

**Workflow**
1. SSH into the server
2. Run `scripts/deploy.sh`
3. The script builds backend + frontend, restarts services, checks health

**Pros**
- Very simple
- Easy to debug
- Minimal infrastructure cost

**Cons**
- Brief downtime on service restart
- Build time on server
- No built‑in rollback

---

## Improvement Path A: Safer Systemd Deploys (No Containers)

**Goal:** Reduce downtime and add rollback without changing the stack.

**Options**
- Keep two binaries (current + previous) and swap symlink on deploy
- Only restart backend if frontend is static or unchanged
- Add explicit rollback command (symlink + restart)
- Prebuild binaries on CI and scp to server

**Pros**
- Minimal new infrastructure
- Easier rollbacks
- Faster deploys if builds move to CI

**Cons**
- Still single instance
- Still restarts on deploy

---

## Improvement Path B: Static Frontend + Caddy (No Dev Server)

**Goal:** Reduce runtime complexity and avoid dev server in production.

**Approach**
- `dx build --release`
- Serve `/opt/choiros/dioxus-desktop/dist` via Caddy `file_server`
- Stop running `dx serve` in production

**Pros**
- Lower CPU/memory
- Fewer running processes
- Standard web deployment

**Cons**
- Hot‑patching requires a separate mechanism

---

## Improvement Path C: Containerized Deployments (Later Phase)

**Goal:** Consistent builds and easier scaling.

**Approach**
- Build images in CI (GHCR or ECR)
- Run via Docker/Podman or ECS
- Separate backend and frontend images

**Pros**
- Environment consistency
- Easier rollbacks (image tags)
- Portable across hosts

**Cons**
- More moving parts
- Slower iteration without good tooling

---

## Improvement Path D: Blue/Green or Rolling (Later Phase)

**Goal:** Near‑zero downtime and safer rollouts.

**Approach**
- Run two instances (blue/green) behind a proxy
- Switch traffic after health checks
- Roll back instantly if issues detected

**Pros**
- Minimal downtime
- Safer deploys

**Cons**
- Requires additional infra
- Higher costs

---

## Decision Guidance

**Right now:** stick with systemd + Caddy and keep it simple.

Upgrade when:
- You have real user traffic that notices downtime
- Builds start taking too long on the server
- You want safer rollbacks
- You’re ready to standardize on containers

---

## Near‑Term Recommendations

- Add a “two‑binary rollback” pattern before containers
- Serve static frontend via Caddy once hot‑patching is defined
- Add CI deploy job once the workflow is stable
- Keep configs env‑driven (DB path, CORS allow‑list)

