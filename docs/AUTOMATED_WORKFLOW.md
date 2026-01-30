# ChoirOS Automated Development Workflow

This directory contains automation for long-running agentic development of ChoirOS.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  OpenProse Orchestrator                                         │
│  (choiros-agentic-build.prose)                                  │
│  - Manages phases (setup → implementation → verify → deploy)   │
│  - Parallel agent sessions for different components             │
│  - Automatic rollback on failure                                │
└──────────────────────────┬──────────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────────┐
│  Tmux Dev Workflow (dev-workflow.sh)                            │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐           │
│  │Sandbox  │  │Dioxus   │  │File     │  │E2E      │           │
│  │API      │  │UI       │  │Watcher  │  │Tests    │           │
│  │:8080    │  │:5173    │  │(auto)   │  │(on cmd) │           │
│  └─────────┘  └─────────┘  └─────────┘  └─────────┘           │
└──────────────────────────┬──────────────────────────────────────┘
                           │ SSH
┌──────────────────────────▼──────────────────────────────────────┐
│  EC2 Development Server (3.83.131.245)                          │
│  - Rust toolchain                                               │
│  - Dioxus + Actix                                               │
│  - SQLite                                                       │
│  - BAML                                                         │
└─────────────────────────────────────────────────────────────────┘
```

## Quick Start

### 1. Start Development Session

```bash
# On EC2 (or locally)
cd ~/choiros-rs
./scripts/dev-workflow.sh start

# This creates a tmux session with 5 windows:
#   0: editor    - Your code editor
#   1: sandbox   - Actix API server (port 8080)
#   2: ui        - Dioxus dev server (port 5173)
#   3: watcher   - Auto-run tests on file change
#   4: e2e       - Browser testing (manual trigger)
#   5: logs      - Monitor all logs
```

### 2. Attach to Session

```bash
./scripts/dev-workflow.sh attach

# Navigation:
#   Ctrl+B N    - Next window
#   Ctrl+B P    - Previous window
#   Ctrl+B D    - Detach (keep running)
#   Ctrl+B [0-5]- Jump to window number
```

### 3. Check Status

```bash
./scripts/dev-workflow.sh status
```

### 4. Create Checkpoint (Safe State)

```bash
./scripts/dev-workflow.sh checkpoint
# Creates git tag: checkpoint-20260130-120000
```

### 5. Rollback on Failure

```bash
./scripts/dev-workflow.sh rollback
# Or rollback to specific checkpoint:
./scripts/dev-workflow.sh rollback checkpoint-20260130-120000
```

## OpenProse Agentic Build

### Phase-by-Phase (Recommended for Testing)

```bash
# Phase 1: Setup only
prose run openprose/choiros-agentic-build.prose phase=setup

# Phase 2: Implementation (parallel agents)
prose run openprose/choiros-agentic-build.prose phase=implementation

# Phase 3: Verification loop
prose run openprose/choiros-agentic-build.prose phase=verification

# Phase 4: Deploy
prose run openprose/choiros-agentic-build.prose phase=deployment
```

### Full Autonomous Mode (Overnight Build)

```bash
# WARNING: This runs unattended for hours
prose run openprose/choiros-agentic-build.prose mode=autonomous max_iterations=10
```

**Safety features:**
- Creates checkpoint every 5 minutes
- Auto-rollback on test failure
- Max 10 iterations before human review
- Logs all activity to `~/.local/share/choiros/logs/`

## Agentic Development Workflow

### 1. Specification-Driven

Agents read from `docs/ARCHITECTURE_SPECIFICATION.md`:
- Component interfaces
- Data flow diagrams
- API contracts
- Event schemas

### 2. Parallel Implementation

Multiple agents work simultaneously:
```
Agent A: EventStoreActor (backend)
Agent B: ChatActor (backend)  
Agent C: Chat UI (frontend)
```

### 3. Verification Loop

After each iteration:
1. Run unit tests
2. Run integration tests
3. Run E2E tests
4. Verify code coverage ≥ 85%
5. Check clippy warnings = 0

If any fail:
- Rollback to last checkpoint
- Generate fix plan
- Apply fixes
- Retry

### 4. Human-in-the-Loop (Optional)

For assisted mode, agents pause for review:
```prose
if mode == "assisted" {
  poll "Approve changes?" options=["Yes", "No", "Modify"]
  if selected == "Modify" {
    input "What changes?" → human_feedback
    agent apply_changes feedback=human_feedback
  }
}
```

## Rollback Strategy

### Automatic Rollbacks

```rust
// On test failure
if test_results.pass_rate < 0.85 {
  git reset --hard checkpoint-last-good
  restart_services()
  retry_with_fixes()
}
```

### Manual Rollbacks

```bash
# See all checkpoints
git tag | grep checkpoint

# Rollback to specific
git reset --hard checkpoint-20260130-120000
./scripts/dev-workflow.sh restart
```

## Monitoring

### Live Logs

```bash
# All services
multitail ~/.local/share/choiros/logs/*.log

# Specific service
tail -f ~/.local/share/choiros/logs/sandbox.log
```

### Health Checks

```bash
# API health
curl http://localhost:8080/health

# UI available
curl -I http://localhost:5173
```

### Metrics

```bash
# Test coverage
cargo tarpaulin --out Html

# Build times
cargo build --release --timings
```

## Troubleshooting

### Tmux Session Lost

```bash
# Reattach
tmux attach -t choiros-dev

# Or list all sessions
tmux ls
```

### Port Conflicts

```bash
# Find what's using port 8080
lsof -i :8080

# Kill process or change ports in Justfile
```

### Disk Full

```bash
# Clean cargo cache
cargo clean

# Clean old logs
rm ~/.local/share/choiros/logs/*.log.*
```

## Advanced Usage

### Custom Agent Configuration

Edit `openprose/choiros-agentic-build.prose`:
```prose
let config = {
  max_iterations: 20,        # More retries
  test_threshold: 0.90,      # Higher quality bar
  checkpoint_interval: 10,   # Less frequent checkpoints
  rollback_on_failure: false, # Debug mode
}
```

### Selective Component Build

```bash
# Build only backend
prose run openprose/choiros-agentic-build.prose components=["event-store", "chat-actor"]

# Build only UI
prose run openprose/choiros-agentic-build.prose components=["chat-ui"]
```

### Continuous Integration

```bash
# Add to crontab for nightly builds
0 2 * * * cd ~/choiros-rs && prose run openprose/choiros-agentic-build.prose mode=autonomous
```

## Safety Checklist

Before running autonomous mode:

- [ ] Initial checkpoint created
- [ ] Git repo initialized
- [ ] EC2 has sufficient disk space (>10GB free)
- [ ] SSH key working
- [ ] Logs directory writable
- [ ] Max iterations set reasonably (5-10)
- [ ] Rollback enabled
- [ ] Human notified of start time

## Success Metrics

After autonomous build completes:

- [ ] All tests pass
- [ ] Code coverage ≥ 85%
- [ ] No clippy warnings
- [ ] Production build works
- [ ] Final checkpoint tagged
- [ ] Log files reviewed for errors

## Next Steps

1. **Manual Development**: Use `./dev-workflow.sh` for interactive coding
2. **Assisted Automation**: Run OpenProse with `mode=assisted` for human review
3. **Full Autonomy**: Run overnight with `mode=autonomous`
4. **Scale**: Deploy to production with `just deploy-ec2`

---

**Questions?** Check `docs/ARCHITECTURE_SPECIFICATION.md` for detailed component specs.
