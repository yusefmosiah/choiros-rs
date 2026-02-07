# Documentation Cleanup & React Removal Guide

**Date:** 2026-02-06  
**Status:** Ready for Execution  
**Scope:** Archive React migration docs, remove sandbox-ui code, update documentation

---

## Executive Summary

This document provides step-by-step instructions to clean up documentation and remove all traces of the abandoned Dioxus-to-React migration. 

**Status Update (2026-02-06):** The ChoirOS supervision cutover is now **COMPLETE** (see `docs/architecture/supervision-cutover-handoff.md`). The team has successfully migrated from ActorManager-based architecture to a proper ractor supervision tree. The React migration attempt (Phase 2, ~50 commits over 3 days) was abandoned due to critical CPU regressions. The focus is now on **multiagent architecture** with supervision foundation in place.

### What Happened

1. **Initial Migration (Jan 30 - Feb 2):** Migrated frontend from Dioxus 0.7 to React 18 + TypeScript + Vite
   - Achieved feature parity for Desktop, WindowManager, Chat, Terminal
   - Implemented ts-rs type generation from Rust
   - Created Zustand state management
   - Fixed multiple bugs (duplicate window creation, WebSocket race conditions, StrictMode issues)
   - 33 frontend tests passing

2. **Regressions Discovered (Feb 5-6):**
   - CPU spikes with terminal windows, especially on reload or multi-browser
   - Desktop loading deadlock on WebSocket startup failures
   - Terminal connection reliability issues

3. **Rollback Decision (Feb 6):**
   - Kept `dioxus-desktop/` as the active frontend
   - `sandbox-ui/` (React) kept as backup during validation
   - Returned to fixing Dioxus issues (WebSocket stabilization, terminal multi-browser fixes)

4. **Supervision Cutover COMPLETE (Feb 6):**
   - **NEW:** Successfully migrated to ractor supervision tree
   - Removed ActorManager anti-patterns (DashMap, Mutex)
   - All validation gates passing
   - Ready for multiagent rollout (ResearcherActor, DocsUpdaterActor, VerifierAgent, WatcherActors)
   - See `docs/architecture/supervision-cutover-handoff.md` for full details

**Current Architecture Status:**
```
✅ dioxus-desktop/    - Active frontend (Dioxus 0.7)
✅ sandbox-ui/          - Backup (React) - TO BE REMOVED
✅ Supervision tree      - COMPLETE (ractor-based)
✅ Multiagent rollout   - NEXT PHASE (per design doc)
```

---

## Current Priority: Supervision Cutover Cleanup

**IMPORTANT:** The primary priority is now **multiagent rollout**, not React cleanup. React removal is housekeeping.

**Rationale:**
- Supervision cutover is **COMPLETE** (all validation gates passing)
- Multiagent architecture is **NEXT PHASE** (VerifierAgent, FixerActor, ResearcherActor, etc.)
- React cleanup is low-risk cleanup, not blocking work

**Recommended Approach:**
1. **Optional:** Perform React cleanup in parallel with multiagent work
2. **Alternative:** Defer React cleanup until multiagent Phase 1 complete
3. **Fast-path:** Quick cleanup now (archive 3 docs, delete sandbox-ui/, update 2 files)

**Decision Point:** Choose based on available time/tokens.

---

## Files to Archive

### 1. Migration Documentation

**File:** `docs/dioxus-to-react.md`  
**Action:** Move to `docs/archive/dioxus-to-react.md`  
**Rationale:** Comprehensive 500-line migration plan is now historical artifact

**File:** `docs/handoffs/2026-02-06-react-terminal-browser-cpu-regression.md`  
**Action:** Move to `docs/archive/2026-02-06-react-terminal-browser-cpu-regression.md`  
**Rationale:** Documents the regression that caused rollback - preserve for future reference

**File:** `docs/porting-fixes-required.md`  
**Action:** Move to `docs/archive/porting-fixes-required.md`  
**Rationale:** 65% feature parity report for abandoned React implementation

### 2. Additional Docs with React References (Review & Update)

These files contain React references that need to be updated or removed:

- `README.md` - Says `sandbox-ui/` is "Dioxus frontend (WIP)" - INCORRECT, it's React
- `Justfile` - Has `dev-ui-react` command that should be removed
- `AGENTS.md` - Check for React references
- `CLAUDE.md` - Check for React references

---

## Code to Remove

### sandbox-ui/ Directory (React Frontend)

**Location:** `/Users/wiz/choiros-rs/sandbox-ui/`  
**Action:** Delete entire directory  
**Rationale:** Abandoned React implementation, dioxus-desktop is now the official frontend

**Before deletion, verify:**
- All React code has been reviewed and is no longer needed
- No unique implementations worth porting to Dioxus
- Backup created if needed (git archive)

### Deletion Commands

```bash
# Verify what will be deleted
ls -la sandbox-ui/

# Optional: Create a backup before deletion
git archive --format=tar --prefix=sandbox-ui-backup/ HEAD sandbox-ui/ | gzip > sandbox-ui-backup.tar.gz

# Delete the directory
rm -rf sandbox-ui/

# Remove React references from .gitignore if any
grep -n "sandbox-ui" .gitignore
```

---

## Documentation Updates Required

### 1. README.md

**Current Issues:**
- Line 90: `| Frontend | Dioxus 0.7 (WASM) |` - Correct
- Line 110: `├── sandbox-ui/             # Dioxus frontend (WIP)` - INCORRECT, should reference `dioxus-desktop/`

**Additional Updates (Supervision Cutover):**
- Update to reflect supervision tree structure
- Reference `docs/architecture/supervision-cutover-handoff.md` for architecture details
- Update "Quick Start" commands to match new supervision paths (if needed)

**Required Changes:**
```markdown
├── dioxus-desktop/         # Dioxus 0.7 frontend (WASM)
# Remove sandbox-ui reference
```

### 2. Justfile

**Current Issues:**
- Lines 21-23: `dev-ui-react` recipe should be removed
- Lines 22-23: References `sandbox-ui` directory

**Required Changes:**
```makefile
# REMOVE THESE LINES (21-23):
# dev-ui-react:
#     cd sandbox-ui && npm run dev -- --port 3000

# UPDATE Line 30 (stop recipe):
@pkill -9 -f "vite --port 3000" 2>/dev/null || true
# (Can keep this for now, but it's no longer needed)
```

### 3. progress.md

**Add Section After "ChoirOS Progress - 2026-02-06 (Rollback)":**

```markdown
# ChoirOS Progress - 2026-02-06 (Rollback)

## Summary

**Rollback to Dioxus** - After completing Phase 2 of React migration (~50 commits), 
encountered critical CPU regressions with terminal windows and desktop loading deadlocks.
Decision made to revert to Dioxus frontend while investigating root cause.

## Regression Issues

1. **Terminal CPU Regression**
   - Browser CPU spiked with terminal windows
   - Exacerbated by page reloads and multi-browser sessions
   - ResizeObserver feedback loop causing excessive render churn
   - Fixed in React but fundamental architecture issue remained

2. **Desktop Loading Deadlock**
   - UI stuck on "Loading desktop..." when WebSocket startup failed
   - No timeout or fallback mechanism
   - Added 8-second timeout but issue persisted

## Rollback Decision

**Decision:** Keep `dioxus-desktop/` as active frontend, archive `sandbox-ui/` (React)

**Rationale:**
- Dioxus has stable WebSocket implementation
- Terminal multi-browser/reload stability issues were fixable in Dioxus
- React implementation had architectural issues (state duplication, complex event handling)
- Development velocity higher with proven Dioxus codebase

## Fixes Since Rollback

- WebSocket stabilization: Replaced direct signal mutation with queued event processing
- Terminal connection reliability: Added watchdog timeout, improved event sequencing
- Window drag behavior: Moved to pointer lifecycle events (pointerdown/move/up)

## Next Steps

1. Complete terminal multi-browser/reload stabilization
2. Fix window drag release semantics
3. Continue feature development in Dioxus

---

*Last updated: 2026-02-06*
*Status: Rolled back to Dioxus, React archived*
```

### 4. AGENTS.md

**Review for React References:**

Search for:
- `sandbox-ui` references
- `React` or `react` mentions
- `TypeScript` or `TS` mentions (unless in context of other tools)

Update to reflect `dioxus-desktop` as the official frontend.

---

## Verification Checklist

After completing cleanup:

- [ ] `docs/dioxus-to-react.md` moved to `docs/archive/`
- [ ] `docs/handoffs/2026-02-06-react-terminal-browser-cpu-regression.md` moved to `docs/archive/`
- [ ] `docs/porting-fixes-required.md` moved to `docs/archive/`
- [ ] `sandbox-ui/` directory deleted
- [ ] `README.md` updated to reference `dioxus-desktop/`
- [ ] `Justfile` `dev-ui-react` recipe removed
- [ ] `progress.md` updated with rollback section
- [ ] `AGENTS.md` reviewed and updated for React references
- [ ] No React dependencies remain in workspace
- [ ] Frontend dev commands reference `dioxus-desktop` only
- [ ] Build commands updated if they referenced React

---

## Archive Structure After Cleanup

```
docs/
├── archive/
│   ├── dioxus-to-react.md
│   ├── 2026-02-06-react-terminal-browser-cpu-regression.md
│   └── porting-fixes-required.md
├── architecture/
├── handoffs/
├── research/
└── ...
```

---

## Current Project Structure (After Cleanup)

```
choiros-rs/
├── Cargo.toml              # Workspace definition
├── Justfile
├── dioxus-desktop/         # Dioxus 0.7 frontend (OFFICIAL)
├── sandbox/                # Rust backend (Axum + Ractor)
├── hypervisor/             # Edge router
├── shared-types/           # Shared types
├── skills/                 # AI agent skills
└── docs/                   # Documentation
```

---

## Summary of Progress

### What Was Accomplished (React Migration - Now Archived)

**Phase 1 Complete:** Type generation with ts-rs
**Phase 2 Complete:** Core infrastructure (React + Vite + TypeScript, Zustand stores, WebSocket client)
**Phase 2 Complete:** UI components (Desktop, WindowManager, Chat, Terminal)
**Phase 2 Complete:** Bug fixes (duplicate windows, WebSocket race conditions, StrictMode)
**Tests:** 33 frontend tests passing

### Why Rollback

**Regressions:**
- Terminal CPU spikes (especially with reload/multi-browser)
- Desktop loading deadlock
- Architectural complexity (state duplication, complex event handling)

### Current Focus

**Active Frontend:** `dioxus-desktop/`
**Priority Fixes:**
- WebSocket stabilization
- Terminal multi-browser/reload reliability
- Window drag behavior

**Next Features:**
- Chat thread management
- File browser improvements
- Settings panel

---

## Commands to Execute Cleanup

```bash
# Step 1: Create archive directory
mkdir -p docs/archive

# Step 2: Move docs to archive
mv docs/dioxus-to-react.md docs/archive/
mv docs/handoffs/2026-02-06-react-terminal-browser-cpu-regression.md docs/archive/
mv docs/porting-fixes-required.md docs/archive/

# Step 3: Verify React code is ready to delete
ls -la sandbox-ui/

# Step 4: Create backup (optional)
git archive --format=tar --prefix=sandbox-ui-backup/ HEAD sandbox-ui/ | gzip > sandbox-ui-backup.tar.gz

# Step 5: Delete sandbox-ui
rm -rf sandbox-ui/

# Step 6: Update README.md (manual edit required)
# Update line 110 to reference dioxus-desktop instead of sandbox-ui

# Step 7: Update Justfile (manual edit required)
# Remove dev-ui-react recipe (lines 21-23)

# Step 8: Update progress.md (manual edit required)
# Add rollback section after 2026-02-06 section

# Step 9: Review AGENTS.md for React references
grep -n "react\|React\|sandbox-ui\|TypeScript" AGENTS.md

# Step 10: Review CLAUDE.md for React references
grep -n "react\|React\|sandbox-ui\|TypeScript" CLAUDE.md

# Step 11: Verify no React references remain
grep -r "react\|React" docs/ --include="*.md" | grep -v "archive/" | grep -v "node_modules"
```

---

## Notes for Future Reference

**Lessons Learned:**
1. Dioxus has advantages for Rust-based projects (no type bridge, unified language)
2. React migration revealed performance issues with complex state management
3. WebSocket stability is critical and harder to achieve in multi-language stacks
4. Feature parity assessments (porting-fixes-required.md) revealed 35% gap

**Preserved Knowledge:**
- Type generation techniques (ts-rs) may be useful for other integrations
- WebSocket client patterns documented in React implementation
- Testing strategies (33 frontend tests) can be applied to Dioxus

**Decision Record:**
- Date: 2026-02-06
- Decision: Rollback to Dioxus, archive React implementation
- Drivers: CPU regression, deadlock, architectural complexity
- Alternatives considered: Fix React issues (too complex), hybrid approach (rejected)

---

*Last updated: 2026-02-06*
*Status: Ready for Execution*

### 5. AGENTS.md

**Recommended Updates:**

**After Supervision Cutover:**
- Review for any references to ActorManager patterns
- Update to reflect supervision tree communication patterns
- Add supervisor message protocol documentation
- Document restart strategies (one_for_one, simple_one_for_one)
- Update examples to use supervisor RPC calls instead of ActorManager

**After Multiagent Rollout:**
- Add VerifierAgent documentation (pipelining, sandbox isolation)
- Add FixerAgent documentation (hotfix strategy, E2E reconciliation)
- Add ResearcherActor documentation (web search, LLM inference)
- Add DocsUpdaterActor documentation (in-memory index, system queries)
- Update communication patterns for multi-agent coordination

**Key Sections to Update:**
- "Task Concurrency" section - already has supervision rules
- "Code Style Guidelines" - add actor naming conventions
- "Architecture Overview" - reference supervision tree structure
- Add new section: "Multi-Agent Coordination"
- Add new section: "Verification & Pipelining"


---

## Updated Priority: Multiagent Rollout First

**Status Change (2026-02-06):**

| Priority | Previous | Current |
|---------|-----------|---------|
| **1** | React cleanup | Multiagent rollout |
| **2** | Docs cleanup | Multiagent rollout |
| **3** | Documentation updates | Supervision refinement (if needed) |

**Rationale:**

1. **Supervision Cutover is COMPLETE** ✅
   - All validation gates passing
   - Multiagent rollout can proceed
   - React cleanup is non-blocking

2. **Architecture Foundation is SOLID** ✅
   - Supervision tree in place
   - EventStore functional
   - Ready for service actors (Researcher, Docs, Watcher, Verifier, Fixer)

3. **Token Efficiency** 💡
   - Focus high-value work (multiagent architecture)
   - React cleanup can be deferred or done in parallel
   - Don't burn tokens on low-priority cleanup

**Recommended Execution:**

**Option A: Quick React Cleanup (Fast-path)**
```bash
# 10-15 minutes, clears deck for multiagent work
mkdir -p docs/archive
mv docs/dioxus-to-react.md docs/handoffs/2026-02-06-react-terminal-browser-cpu-regression.md docs/porting-fixes-required.md docs/archive/
rm -rf sandbox-ui/
# Update README.md (1 line) and Justfile (1 line)
# Done!
```

**Option B: Parallel Approach**
```bash
# Terminal 1: Start multiagent design work
cd docs/design/
vim 2026-02-06-multiagent-architecture-design.md

# Terminal 2: React cleanup (when switching tasks)
mkdir -p docs/archive
mv docs/dioxus-to-react.md docs/handoffs/2026-02-06-react-terminal-browser-cpu-regression.md docs/porting-fixes-required.md docs/archive/
rm -rf sandbox-ui/
```

**Option C: Defer React Cleanup**
- Focus entirely on multiagent rollout
- React cleanup as lower-priority task
- Return to cleanup when multiagent Phase 1-2 complete

**Recommendation: Option A** - Quick cleanup clears deck for focused multiagent work.

---

## References

1. **Supervision Cutover:** `docs/architecture/supervision-cutover-handoff.md`
2. **Multiagent Architecture:** `docs/design/2026-02-06-multiagent-architecture-design.md`
3. **Original Cleanup Guide:** This document (now updated with priority clarification)
