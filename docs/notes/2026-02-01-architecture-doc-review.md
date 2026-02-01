# Architecture Documentation Review - 2026-02-01

**Reviewer:** nano documentation writer  
**Source:** `/docs/ARCHITECTURE_SPECIFICATION.md` (v1.0)  
**Repo State:** Current as of 2026-02-01

---

## Executive Summary

The architecture specification is **significantly outdated** (written before implementation). Many sections describe aspirational/planned features rather than actual implementation. Key discrepancies exist in technology stack, deployment architecture, and component specifications.

---

## Section-by-Section Accuracy Assessment

### 1. System Overview - PARTIALLY ACCURATE

**Accurate:**
- Core philosophy (actor-owned state, UI as projection) - matches implementation
- Event sourcing approach - implemented in `EventStoreActor`
- Technology stack table format

**Inaccurate:**
- **Sprites.dev** listed as sandbox technology - NOT IMPLEMENTED
- No evidence of Sprites.dev integration in codebase
- Docker mentioned but no Dockerfile exists in repo

### 2. Architecture Diagram - OUTDATED

**Issues:**
- Shows Hypervisor on port 8001 - actual sandbox runs on port 8080
- Shows multiple sandbox instances (9001, 9002, 9003) - not implemented
- Hypervisor is a placeholder (5 lines in `hypervisor/src/main.rs`)
- No multi-tenant routing implemented

**Actual State:**
- Single sandbox binary runs on port 8080
- No hypervisor routing layer active
- Direct connection to sandbox only

### 3. Component Specifications - MIXED ACCURACY

#### 3.1 Hypervisor - NOT IMPLEMENTED
- **Spec claims:** WebAuthn/Passkey, route to sandbox, spawn/kill
- **Actual:** Placeholder main.rs with println! statement only
- **Status:** Stub implementation

#### 3.2 Sandbox - MOSTLY ACCURATE
- **Accurate:** Actix Web server, EventStoreActor, ChatActor exist
- **Inaccurate:** 
  - No `WriterActor` (desktop actor handles windows instead)
  - No `BamlActor` (BAML client is direct integration, not an actor)
  - No `ToolExecutor` actor (tools are in `tools/` module, not actors)
  - Static files not served from `./static` (Dioxus dev server on :3000)

#### 3.3 EventStore Actor - ACCURATE
- Implementation matches spec closely
- Uses `libsql` (not `sqlx` as implied by workspace deps)
- Schema matches specification

#### 3.4 ChatActor - PARTIALLY ACCURATE
- Exists but additional `ChatAgent` actor also present
- No `RunTool` message (tool execution handled differently)
- State storage uses events, not separate `chat_messages` table

#### 3.5 Dioxus UI Components - NOT VERIFIABLE
- Spec shows example code patterns
- Actual UI implementation not reviewed in detail

### 4. Data Flow - CONCEPTUALLY ACCURATE

- Flow descriptions match architectural intent
- Actual implementation may vary in details
- WebSocket push confirmed in `websocket_chat.rs`

### 5. API Contracts - PARTIALLY IMPLEMENTED

**Implemented:**
- `/health` endpoint exists
- WebSocket at `/ws/chat/{actor_id}` exists
- Desktop API endpoints exist (GET/POST/PATCH/DELETE)

**Not Implemented:**
- `/api/chat/send` - uses WebSocket instead
- `/api/chat/messages` - uses WebSocket instead
- `/api/actor/query` - not found
- Generic actor query pattern not implemented

### 6. Event Contract - ACCURATE

- Event schema matches `shared_types::Event`
- Event types defined in constants
- ULID usage confirmed

### 7. Deployment Architecture - MOSTLY OUTDATED

#### 7.1 Production (AWS) - NOT IMPLEMENTED
- **Spec shows:** EC2 with Docker, hypervisor container, sandbox containers
- **Actual:** 
  - EC2 IP (3.83.131.245) referenced in Justfile
  - No Docker Compose file exists
  - No sprites-adapter container
  - Single binary deployment, not containerized

#### 7.2 Docker Compose - NOT IMPLEMENTED
- No `docker-compose.yml` file exists
- No sprites-adapter service
- Docker build commands in Justfile but no Dockerfile in repo

#### 7.3 Environment Variables - PARTIALLY ACCURATE
- `DATABASE_URL` used correctly
- `CHOIR_BAML_PROVIDER` mentioned but not found in code
- `SPRITES_API_TOKEN` referenced in spec but Sprites.dev not used

### 8. Development Workflow - ACCURATE

- Commands match Justfile implementation
- `just dev-sandbox`, `just dev-ui`, `just dev-hypervisor` all exist
- Port 3000 for UI, 8080 for sandbox confirmed

### 9. Testing Strategy - IMPLEMENTED

- Unit test examples match actual test patterns
- Integration tests exist in `sandbox/tests/`
- Test files: `chat_api_test.rs`, `desktop_api_test.rs`, `websocket_chat_test.rs`, etc.
- No Playwright/E2E tests found (spec mentions them)

### 10. CI/CD Pipeline - NOT IMPLEMENTED

- No `.github/workflows/` directory exists
- No CI configuration found
- Deploy commands in Justfile but no automated pipeline

### 11. Observability - PARTIALLY IMPLEMENTED

- `tracing` is used throughout codebase
- Structured logging present
- No Prometheus metrics endpoint found
- No health check with database/actor status (simple "healthy" response only)

### 12. Security Model - NOT IMPLEMENTED

- WebAuthn/Passkey - not implemented
- Capability-based authorization (`Membrane` struct) - not found
- JWT mentioned but not implemented
- No authentication layer active

### 13. BAML Integration - VERSION MISMATCH

**Spec claims:** `baml = "0.218.0"`
**Actual:** 
- Workspace: `baml = "0.218"`
- Generator: `version "0.217.0"` (in `baml_src/generators.baml`)

**Integration:**
- Native Rust BAML client confirmed working
- AWS Bedrock and Z.ai clients configured
- No `BamlActor` - direct integration in chat_agent.rs

### 14. Open Questions - OUTDATED

All questions marked "TBD" or "Decided" have been resolved through implementation:
- Dioxus chosen (confirmed)
- WebSocket for real-time (confirmed)
- Native BAML in Rust (confirmed working)

---

## Claims Requiring Updates

### Critical (Wrong/Misleading)

1. **Sprites.dev** - Remove all references. Not used, never implemented.
2. **Hypervisor** - Clarify it's a placeholder/stub, not functional.
3. **Multi-tenant architecture** - Currently single-tenant only.
4. **Docker deployment** - Not implemented, remove or mark as future.
5. **WebAuthn/Passkey** - Not implemented, remove or mark as future.
6. **CI/CD Pipeline** - Not implemented, remove section.

### Major (Partially Wrong)

7. **Actor list** - Update to actual actors: `ChatActor`, `ChatAgent`, `DesktopActor`, `EventStoreActor`
8. **API endpoints** - Document actual REST + WebSocket hybrid approach
9. **BAML version** - Update to actual versions (0.218 workspace, 0.217 generator)
10. **Database** - Clarify uses `libsql` not `sqlx` for SQLite

### Minor (Clarifications Needed)

11. **Port numbers** - Update to actual ports (8080 sandbox, 3000 UI)
12. **Tool system** - Document as module, not actor
13. **Testing** - Remove Playwright references, document actual test setup

---

## Missing Sections

### Skills System
- **Location:** `skills/` directory
- **Components:**
  - `multi-terminal/` - Terminal session management
  - `session-handoff/` - Context preservation for multi-session workflows
  - `actorcode/` - Research task management system
- **Impact:** Major part of development workflow not documented

### Handoff System
- **Location:** `docs/handoffs/`
- **Purpose:** Multi-session agent context preservation
- **Not mentioned** in architecture spec

### ActorCode Research System
- **Location:** `skills/actorcode/`
- **Features:**
  - Research task launching
  - Findings database
  - Dashboard (tmux + web)
  - Test hygiene checking
- **Not documented** in architecture

### Desktop Window Manager
- **Implemented:** `DesktopActor` manages windows
- **Spec mentions:** WriterActor (not implemented)
- **Gap:** Desktop architecture not covered

### BAML Client Structure
- **Location:** `sandbox/src/baml_client/`
- **Generated code** from BAML files
- **Not documented** in component specs

---

## Repo-Truth Verification Results

### Technology Stack Verification

| Spec Claim | Actual | Status |
|------------|--------|--------|
| Dioxus (WASM) | dioxus = "0.7" | ✅ MATCH |
| Actix (Actors) | actix = "0.13" | ✅ MATCH |
| SQLite | libsql = "0.9" | ⚠️ DIFFERENT (not sqlx) |
| BAML | baml = "0.218" | ⚠️ VERSION MISMATCH |
| Sprites.dev | NOT USED | ❌ WRONG |
| WebSocket + HTTP | actix-ws = "0.2" | ✅ MATCH |

### File Structure Verification

| Spec Path | Exists | Notes |
|-----------|--------|-------|
| `hypervisor/src/main.rs` | ✅ | Placeholder only |
| `sandbox/src/main.rs` | ✅ | Full implementation |
| `sandbox/src/actors/` | ✅ | 4 actor modules |
| `sandbox/src/baml_client/` | ✅ | Generated code |
| `sandbox/static/` | ❌ | Not used (Dioxus dev server) |
| `docker-compose.yml` | ❌ | Not implemented |
| `.github/workflows/ci.yml` | ❌ | Not implemented |
| `baml/` | ❌ | Uses `baml_src/` instead |

### Dependency Verification

**Workspace Cargo.toml:**
- All claimed dependencies present
- Additional: `libsql` (not in workspace deps, in sandbox)
- `sqlx` in workspace but may not be actively used

**Missing from spec:**
- `libsql` - actual database driver
- `dashmap` - concurrent hash map
- `walkdir` - directory traversal
- `pulldown-cmark` - markdown parsing
- `actix-web-actors` - WebSocket actor integration

---

## Recommendations

### Immediate Actions

1. **Create v1.1 Architecture Spec** addressing critical inaccuracies
2. **Remove Sprites.dev references** or document as "evaluated but not adopted"
3. **Document actual deployment** (EC2 + systemd or similar)
4. **Add Skills System section** - significant part of codebase

### Documentation Priorities

1. **High:** Update technology stack table (libsql, remove Sprites.dev)
2. **High:** Clarify hypervisor is stub/not implemented
3. **Medium:** Document actual actor list and responsibilities
4. **Medium:** Add Desktop/Window Manager architecture
5. **Low:** Document BAML client generation workflow

### Architecture Decisions to Document

1. Why libsql over sqlx?
2. Why no hypervisor in current deployment?
3. Why direct BAML integration vs BamlActor?
4. Skills system design rationale

---

## Conclusion

The architecture specification was written as a design document before implementation. While the core concepts (actors, event sourcing, Dioxus) are accurate, many implementation details diverged:

- **Sprites.dev was never adopted**
- **Hypervisor remains unimplemented**
- **Single-tenant deployment** vs multi-tenant design
- **Skills system emerged** as major component

**Recommendation:** Treat spec as historical design doc, create new "Architecture Overview" reflecting actual implementation.

---

*Review completed: 2026-02-01*
