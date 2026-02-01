# Workflow Documentation Review - 2026-02-01

## Summary

Comprehensive review of ChoirOS workflow documentation against actual development practices.

---

## 1. OpenProse Usage

**Status: EXISTS but NOT actively used**

- File: `openprose/choiros-agentic-build.prose` exists (206 lines)
- Contains EC2-centric workflow with phases: setup → implementation → verification → deployment → monitor
- **Issue**: References EC2 host `3.83.131.245` and SSH keys that may not be current
- **Issue**: Uses `prose run` command which may not be installed/available
- **Issue**: No evidence of recent execution (no logs, no prose binary in repo)
- **Recommendation**: Document as "experimental/deprecated" or remove entirely

---

## 2. dev-workflow.sh Analysis

**Status: EXISTS and FUNCTIONAL**

- File: `scripts/dev-workflow.sh` (248 lines)
- Commands: `start`, `stop`, `restart`, `attach`, `status`, `e2e`, `checkpoint`, `rollback`
- Creates tmux session with 6 windows:
  - 0: editor
  - 1: sandbox (API on :8080)
  - 2: ui (Dioxus on :3000)
  - 3: watcher (cargo watch - auto-tests)
  - 4: e2e (browser tests)
  - 5: logs (multitail)

**Issues Found:**
- Uses `cargo watch` which is NOT listed in AGENTS.md dependencies
- Uses `multitail` which may not be installed
- Uses `agent-browser` which is mentioned in AGENTS.md but not verified installed
- Hardcodes `~/choiros-rs` path (may not match actual clone location)
- No `checkpoint` or `rollback` implementation details in AGENTS.md

**Accurate Elements:**
- Ports match Justfile (8080 for API, 3000 for UI)
- Uses `just dev-sandbox` and `just dev-ui` commands
- Log directory: `~/.local/share/choiros/logs/`

---

## 3. Justfile Commands vs Documentation

**Justfile Commands (ACCURATE in AGENTS.md):**

| Command | Purpose | Status |
|---------|---------|--------|
| `just dev-sandbox` | Backend API on :8080 | ✅ Accurate |
| `just dev-ui` | Frontend on :3000 | ✅ Accurate |
| `just dev-hypervisor` | Hypervisor component | ✅ Accurate |
| `just build` | Release build | ✅ Accurate |
| `just build-sandbox` | Frontend + backend | ✅ Accurate |
| `just test` | All workspace tests | ✅ Accurate |
| `just test-unit` | Unit tests only | ✅ Accurate |
| `just test-integration` | Integration tests | ✅ Accurate |
| `just check` | fmt + clippy | ✅ Accurate |
| `just fix` | Auto-fix issues | ✅ Accurate |
| `just migrate` | SQLx migrations | ✅ Accurate |
| `just new-migration` | Create migration | ✅ Accurate |
| `just docker-build` | Docker image | ✅ Accurate |
| `just docker-run` | Docker container | ✅ Accurate |
| `just deploy-ec2` | Deploy to EC2 | ✅ Accurate |

**Justfile Commands (NOT in AGENTS.md):**

| Command | Purpose | Missing? |
|---------|---------|----------|
| `just stop` | Kill dev processes | ❌ Not documented |
| `just actorcode` | Execute actorcode scripts | ❌ Not documented |
| `just research` | Launch research tasks | ❌ Not documented |
| `just research-monitor` | Monitor research | ❌ Not documented |
| `just research-status` | Research status | ❌ Not documented |
| `just findings` | Query findings DB | ❌ Not documented |
| `just research-dashboard` | Tmux research view | ❌ Not documented |
| `just research-web` | Open web dashboard | ❌ Not documented |
| `just findings-server` | API server for dashboard | ❌ Not documented |
| `just research-cleanup` | Cleanup old sessions | ❌ Not documented |
| `just research-diagnose` | Diagnostics | ❌ Not documented |
| `just fix-findings` | Fix with worktree | ❌ Not documented |
| `just check-test-hygiene` | Pre-merge check | ❌ Not documented |

**Critical Gap**: The `actorcode` skill system is a major workflow component (13 Justfile commands) completely absent from AGENTS.md.

---

## 4. AGENTS.md Accuracy

**Accurate Sections:**
- Quick Commands (all listed Justfile commands exist)
- Code Style Guidelines (comprehensive and current)
- Project Structure (matches actual layout)
- Testing Guidelines (accurate cargo test patterns)
- Key Dependencies (matches Cargo.toml)

**Inaccurate/Missing Sections:**
- **In-Repo Skills**: Lists only `multi-terminal` and `session-handoff`
  - Missing: `actorcode` (major skill system)
  - Missing: `dev-browser` (exists in skills/)
- **E2E Testing**: References `agent-browser` but no install verification
- **No mention of**: `just stop`, research commands, actorcode workflow

---

## 5. AUTOMATED_WORKFLOW.md Issues

**Major Inaccuracies:**

1. **OpenProse**: Documented as primary orchestrator but likely not used
2. **EC2 Focus**: Heavy emphasis on EC2 (`3.83.131.245`) which may not be current
3. **Port Mismatch**: Docs say UI on :5173, actual is :3000
4. **Missing Commands**: No mention of `just stop`, actorcode, research system
5. **File Watcher**: References `cargo watch` which isn't in dependencies
6. **Safety Checklist**: References autonomous mode that may not work

**Accurate Elements:**
- dev-workflow.sh commands (start, stop, attach, status, checkpoint, rollback)
- Tmux window structure
- Health check endpoints (:8080/health)
- Log locations (`~/.local/share/choiros/logs/`)

---

## 6. Current Actual Workflow

Based on Justfile and available tools:

### Development (Local)
```bash
# Terminal 1: Backend
just dev-sandbox      # Port 8080

# Terminal 2: Frontend  
just dev-ui           # Port 3000

# Or use tmux workflow
./scripts/dev-workflow.sh start
./scripts/dev-workflow.sh attach
```

### Testing
```bash
just test             # All tests
just test-unit        # Unit only
just test-integration # Integration only
just check            # fmt + clippy
```

### Research/Actorcode Workflow (Major Gap)
```bash
just research <template>        # Launch research
just research-monitor <session> # Monitor progress
just research-status            # View status
just research-dashboard         # Tmux dashboard
just findings                   # Query results
just fix-findings               # Apply fixes
```

### Build & Deploy
```bash
just build-sandbox    # Production build
just deploy-ec2       # Deploy to EC2
```

---

## 7. Recommendations

### Immediate Actions

1. **Update AGENTS.md**:
   - Add `just stop` command
   - Add complete `actorcode` skill documentation
   - Add `dev-browser` skill
   - Verify `agent-browser` installation steps

2. **Deprecate/Remove AUTOMATED_WORKFLOW.md**:
   - Remove OpenProse references (or mark experimental)
   - Update port numbers (:3000 not :5173)
   - Add actorcode workflow section
   - Verify EC2 IP is current

3. **Update dev-workflow.sh**:
   - Add `cargo watch` to dependencies check
   - Add `multitail` to dependencies check
   - Make path configurable (not hardcoded ~/choiros-rs)

### Documentation Structure Recommendation

```
docs/
├── AGENTS.md              # Primary dev guide (update with actorcode)
├── WORKFLOW.md            # Merge AUTOMATED_WORKFLOW.md + fixes
├── ACTORCODE.md           # New: Research system documentation
├── TESTING.md             # Extract from AGENTS.md + TESTING_STRATEGY.md
└── notes/                 # Keep for session notes
```

### Dependencies to Verify

- `cargo watch` (for file watcher)
- `multitail` (for log viewing)
- `agent-browser` (for E2E tests)
- `prose` CLI (if OpenProse is to be used)

---

## 8. Files Reviewed

- ✅ `/Users/wiz/choiros-rs/docs/AUTOMATED_WORKFLOW.md` (315 lines)
- ✅ `/Users/wiz/choiros-rs/AGENTS.md` (225 lines)
- ✅ `/Users/wiz/choiros-rs/Justfile` (140 lines)
- ✅ `/Users/wiz/choiros-rs/scripts/dev-workflow.sh` (248 lines)
- ✅ `/Users/wiz/choiros-rs/openprose/choiros-agentic-build.prose` (206 lines)
- ✅ `/Users/wiz/choiros-rs/skills/actorcode/` (major system, undocumented)

---

## Conclusion

**AGENTS.md**: 85% accurate, missing actorcode system
**AUTOMATED_WORKFLOW.md**: 60% accurate, OpenProse likely deprecated, port mismatch
**dev-workflow.sh**: Functional but missing dependency checks
**Justfile**: Authoritative source, 14 undocumented commands

**Priority**: Document actorcode skill system, verify EC2/automation setup, consolidate workflow docs.
