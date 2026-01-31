# Handoff: Post-libsql Migration - Docs Review Required

**Date:** 2026-01-31  
**Session:** libsql migration complete, docs reconciliation needed  
**Committer:** e649f2b  
**Status:** ✅ Code changes committed, 📋 Documentation review pending

---

## What Just Happened

Successfully migrated the ChoirOS sandbox from sqlx to libsql:
- All build errors resolved
- Server running on localhost:8080
- All 11 unit tests passing
- API endpoints tested and working

**BUT** - discovered significant documentation drift that needs reconciliation.

---

## Critical Finding: Documentation Drift

The imported documentation in `docs/imported/` is **out of sync** with actual implementation:

### 1. Frontend Technology Mismatch
- **Docs say:** Yew (WASM framework)
- **Cargo.toml has:** Dioxus (`dioxus = { version = "0.7", features = ["web", "router"] }`)
- **Actual code:** sandbox-ui/src/lib.rs is empty placeholder

### 2. Documentation Files Status

| File | Status | Issues |
|------|--------|--------|
| CHOIR_RS_FULL_REWRITE_PLAN.md | ⚠️ OUTDATED | References Yew everywhere, mentions 6-hour plan |
| DIOXUS_ACTOR_STATE_ARCHITECTURE.md | ✅ CORRECT | Actually mentions Dioxus (should be primary reference) |
| RUST_BUILD_ORDER.md | ⚠️ OUTDATED | Still references sqlx, old project structure |
| RUST_ACTIX_SUPERVISOR_REWRITE_RESEARCH.md | ⚠️ OUTDATED | Research doc, needs curation |
| AGENT_OS_ARCHITECTURE_SPACE.md | ⚠️ OUTDATED | High-level architecture, needs updating |

### 3. Other Drift Issues
- Build order references sqlx-cli, cargo-nextest (not installed)
- No mention of libsql in any docs
- Project structure in docs doesn't match actual structure
- Various references to Python/BAML that may not be current

---

## Current Implementation State

### ✅ What's Working
1. **libsql migration** - EventStoreActor fully migrated, all tests pass
2. **Actor system** - ChatActor, EventStoreActor, ActorManager all functional
3. **HTTP API** - REST endpoints working:
   - GET /health
   - POST /chat/send
   - GET /chat/{actor_id}/messages
4. **Database** - SQLite with libsql, auto-migrations on startup
5. **Build** - Clean build with only 3 minor warnings

### 📋 What's Next (After Docs Review)
1. Decide: Yew vs Dioxus for frontend
2. Add LLM integration (BAML in Cargo.toml but unused)
3. Add tool calling system
4. WebSocket support for real-time updates
5. Hypervisor component (exists but minimal)

---

## Immediate Next Steps for Docs Review

1. **Read and compare** all docs in `docs/imported/` against actual code
2. **Identify** which docs are still relevant vs. obsolete
3. **Decide** on frontend framework (Yew or Dioxus)
4. **Update or archive** outdated documentation
5. **Create** accurate architecture doc based on current implementation
6. **Document** the actual tech stack and build process

### Key Questions to Answer
- Should we use Yew or Dioxus? (Dioxus is in Cargo.toml, Yew in docs)
- What happened to the 6-hour implementation plan? (Is it still relevant?)
- What's the actual vs. planned architecture?
- Which docs should be kept, updated, or deleted?
- What's the canonical source of truth for architecture decisions?

---

## Technical Context

### Current Tech Stack (Verified)
- **Backend:** Rust + Actix-web + Actix actors
- **Database:** SQLite via libsql (migrated from sqlx)
- **Frontend:** Undecided (Dioxus in deps, Yew in docs)
- **Build:** Cargo workspace with 4 crates (shared-types, hypervisor, sandbox, sandbox-ui)
- **LLM:** BAML crate in dependencies but unused

### Working Commands
```bash
# Build
cargo build -p sandbox

# Test
cargo test -p sandbox

# Run server
./target/debug/sandbox
# Server runs on http://localhost:8080

# Test endpoints
curl http://localhost:8080/health
curl -X POST http://localhost:8080/chat/send \
  -H "Content-Type: application/json" \
  -d '{"actor_id":"test","user_id":"me","text":"hello"}'
```

---

## Files Modified in This Session

### Code Changes (Committed)
- `sandbox/Cargo.toml` - Migrated sqlx → libsql
- `sandbox/src/actors/event_store.rs` - Complete libsql rewrite
- `sandbox/src/actors/mod.rs` - Cleaned up exports
- `sandbox/src/api/chat.rs` - Removed unused imports
- `sandbox/src/main.rs` - Updated database connection
- `progress.md` - Updated with current status

### Documentation (Needs Review)
- `docs/imported/CHOIR_RS_FULL_REWRITE_PLAN.md` - Yew references, 6-hour plan
- `docs/imported/DIOXUS_ACTOR_STATE_ARCHITECTURE.md` - Actually correct about Dioxus
- `docs/imported/RUST_BUILD_ORDER.md` - References old sqlx setup
- `docs/imported/RUST_ACTIX_SUPERVISOR_REWRITE_RESEARCH.md` - Research notes
- `docs/imported/AGENT_OS_ARCHITECTURE_SPACE.md` - High-level architecture

---

## Blockers / Open Questions

1. **Frontend framework decision** - Can't build UI until Yew vs Dioxus is decided
2. **Documentation accuracy** - Risk of following outdated instructions
3. **Architecture alignment** - Ensure all components match documented plan
4. **BAML usage** - Is BAML still part of the plan or should it be removed?

---

## Recommended Next Session

**Focus:** Documentation audit and reconciliation

**Goal:** Produce a single, accurate architecture document that reflects:
- Current implementation (libsql, Actix, Dioxus in deps)
- Actual project structure
- Realistic next steps (not 6-hour fantasy)
- Clear technology choices with rationale

**Output:** Updated docs or new canonical reference doc

---

## Notes for Next Agent

- The code works. The server runs. Tests pass. This is solid ground.
- The problem is documentation drift, not code quality.
- Don't start building new features until docs are reconciled.
- Focus on understanding what the CURRENT architecture is, not what it was planned to be.
- Dioxus is in Cargo.toml but Yew is in docs - this needs resolution.

---

*Created after libsql migration session  
Context: docs/imported/ contains outdated Yew references, actual code uses Dioxus*