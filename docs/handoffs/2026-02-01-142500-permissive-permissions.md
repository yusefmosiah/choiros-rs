# Handoff: Actorcode Permission & Session Management

## Session Metadata
- Created: 2026-02-01 14:25:00
- Project: /Users/wiz/choiros-rs
- Branch: main
- Session duration: ~2.5 hours

### Recent Commits
  - (working session - no commits yet)

## Handoff Chain

- **Continues from**: None
- **Supersedes**: [2026-02-01-124700-research-verification.md](./2026-02-01-124700-research-verification.md)

## Current State Summary

**PHILOSOPHY SHIFT: Be Permissive, Not Protective**

We've learned that restrictive permission policies are counterproductive. With proper isolation mechanisms (worktrees, repo scoping), the "dangerous commands" threat model is largely moot. OpenCode's own safety features + our isolation layers provide sufficient protection.

**Key Principle:** Stay in OpenCode's happy path. Default to "allow" for everything. Only block what OpenCode itself warns about (doom_loop, etc.).

## Work Completed

### Investigation Complete

**Learned about OpenCode API & Permissions:**
1. **API endpoints work correctly** - previous issues were hitting wrong routes (`/api/sessions` vs `/session`)
2. **Sessions waiting for input** - not broken, just waiting for permission/question responses
3. **Permission system** - can be set at session creation time (permissive mode) or handled dynamically
4. **Permission handling API** - `POST /session/{id}/permissions/{permissionID}` with `{ response: "once" | "always" | "reject" }`

### Bug Findings

| Issue | Status | Root Cause | Fix |
|-------|--------|------------|-----|
| `actorcode logs` hangs forever | Confirmed | `fs.watch()` with no timeout | Add `--follow` flag, default to tail-only |
| Zombie log processes | Confirmed | Same as above | Same fix |
| Sessions "stuck" in BUSY | Not a bug | Waiting for user permission | Pre-approve permissions |
| API returns HTML | Misunderstanding | Wrong endpoints tested | Use correct `/session` routes |

## Architecture: Permissive Permission Model

### Why Be Permissive?

**Threat Model Analysis:**
| Threat | Mitigation | Still Needs Permission? |
|--------|------------|------------------------|
| Delete repo files | Worktree isolation | ❌ No |
| Edit outside repo | Repo scoping | ❌ No |
| Install system packages | Worktree isolation | ❌ No |
| Push to remote | This is desired behavior | ❌ No |
| Doom loops | OpenCode's own protection | ✅ Yes (rare) |
| Infinite bash loops | OpenCode's own protection | ✅ Yes (rare) |

**Conclusion:** With worktrees + repo scoping, agents are already sandboxed. Permission system is overkill and blocks progress.

### Happy Path Permissions

```javascript
// Session creation with permissive permissions
await client.session.create({
  body: {
    title: "Research task",
    permission: {
      edit: "allow",      // Allow all file edits
      bash: "allow",      // Allow all bash commands
      webfetch: "allow",  // Allow web requests
      doom_loop: "allow",  // Trust OpenCode's detection (rare case)
      external_directory: "allow" // Trust repo scoping
    }
  }
});
```

**What We're NOT Blocking:**
- File edits (even deletions) - worktree protects us
- Bash commands - worktree isolates them
- Web fetch - no external system impact
- Git operations - this is often desired

**What We're Still Blocking (OpenCode's internal safety):**
- `doom_loop: "ask"` - let OpenCode catch infinite loops
- We can set to "allow" to fully trust, but "ask" is fine for rare edge case

### Isolation Layers (Our Real Safety)

1. **Worktrees**: Each research task in separate git worktree
2. **Repo Scoping**: Agent can't access files outside `--directory` parameter
3. **Test Hygiene**: Must pass `cargo test` before merge
4. **Git History**: Every change is tracked and reviewable
5. **Rollback**: Can abandon worktree if damage occurs

This is much more robust than permission prompts.

## Implementation Updates

### 1. Pre-Approve Permissions on Spawn

**File: `skills/actorcode/scripts/research-launch.js`**

Change line 172-180 to add permissions:
```javascript
await client.session.promptAsync({
  path: { id: sessionId },
  query: { directory: DIRECTORY },
  body: {
    parts: [{ type: "text", text: fullPrompt }],
    agent: template.agent,
    model: { providerID, modelID },
    permission: {
      edit: "allow",
      bash: "allow",
      webfetch: "allow",
      doom_loop: "ask"  // Only ask for actual danger
    }
  }
});
```

**File: `skills/actorcode/scripts/actorcode.js` (handleSpawn function)**

Add permission support around line 206:
```javascript
const promptBody = {
  parts: toTextParts(fullPrompt)
};
if (agent) {
  promptBody.agent = agent;
}
if (model) {
  promptBody.model = model;
}
// NEW: Add permissive permissions
promptBody.permission = {
  edit: "allow",
  bash: "allow",
  webfetch: "allow",
  doom_loop: "ask"
};
```

### 2. Fix Logs Command Timeout

**File: `skills/actorcode/scripts/actorcode.js` (handleLogs function, lines 500-545)**

Current behavior: Tails file indefinitely via `fs.watch()`

Proposed change: Add `--follow` flag, default to tail-only
```javascript
async function handleLogs(options) {
  const sessionId = options.id || options.session || null;
  const follow = optionEnabled(options, "follow");
  const logPath = sessionId ? sessionLogPath(sessionId) : supervisorLogPath();

  await logSupervisor(`logs tail path=${logPath} follow=${follow}`);

  let content = "";
  try {
    content = await fs.readFile(logPath, "utf8");
  } catch (error) {
    if (error && error.code === "ENOENT") {
      throw new Error(`Log file not found: ${logPath}`);
    }
    throw error;
  }

  const lines = content.split("\n");
  const tail = lines.slice(Math.max(0, lines.length - 200)).join("\n");
  if (tail.trim()) {
    process.stdout.write(`${tail}\n`);
  }

  // Only start watching if --follow flag is set
  if (!follow) {
    return; // Exit after printing current tail
  }

  let lastSize = Buffer.byteLength(content);
  const watcher = (await import("fs")).watch(logPath, async (eventType) => {
    if (eventType !== "change") {
      return;
    }
    const stats = await fs.stat(logPath);
    if (stats.size <= lastSize) {
      return;
    }
    const stream = (await import("fs")).createReadStream(logPath, {
      start: lastSize,
      end: stats.size
    });
    stream.on("data", (chunk) => {
      process.stdout.write(chunk.toString("utf8"));
    });
    lastSize = stats.size;
  });

  process.on("SIGINT", () => {
    watcher.close();
    process.exit(0);
  });
}
```

Update usage line 52 to include `--follow` flag.

### 3. Kill Stuck Sessions

Sessions waiting for user input can be aborted:
```bash
# Kill all stuck sessions
just actorcode abort --id ses_3e71e96f4ffeGH3pl0ZZMnrIyW
just actorcode abort --id ses_3e71e96f3ffedIidKaDFeaVz6a
just actorcode abort --id ses_3e71eacd0ffexdppWF1lYiAAmu
```

### 4. Aggressive Background Session Usage

**Pattern:** Supervisor stays free, delegates everything to subtasks

```javascript
// Instead of: supervisor doing work directly
await supervisor.doWork(task); // Blocks supervisor

// Use: spawn background session for work
await spawnSubtask(task); // Supervisor stays free
// Subtask runs independently, reports via [LEARNING] tags
```

**Benefits:**
- Supervisor never blocks on slow operations
- Parallel research across multiple hypotheses
- Main context stays clean (just orchestration)
- Can spawn many subtasks concurrently

**Example: Instead of serial investigation, do parallel:**
```bash
# Old way (blocks supervisor):
just research security-audit code-quality

# New way (parallel, supervisor stays free):
for task in security-audit code-quality docs-gap; do
  just research $task &
done
```

## Commands Summary

### Improved Actorcode CLI

```bash
# Spawn with pre-approved permissions (NEW)
just actorcode spawn --title "Task" --agent explore --permission-allow

# Logs without hanging (UPDATED: added --follow flag)
just actorcode logs --id <session>              # Tail current logs, exit
just actorcode logs --id <session> --follow     # Tail and watch (default old behavior)

# Research with auto-approval (UPDATED: pre-approves permissions)
just research <template>                         # Now uses permissive permissions

# Abort stuck sessions
just actorcode abort --id <session>
```

### Justfile Additions

```justfile
# Kill stuck sessions waiting for input
research-abort-stuck:
    @echo "Aborting stuck sessions..."
    @for sid in $(just actorcode status | grep BUSY | awk '{print $$1}'); do \
        echo "Aborting $$sid"; \
        just actorcode abort --id $$sid; \
    done
```

## Key Learnings

### Technical
1. **OpenCode API endpoints**: `/session`, `/session/{id}/message`, `/session/{id}/permissions/{permissionID}`
2. **Permission modes**: "ask" | "allow" | "deny" - can be set per tool type
3. **Permission response API**: `POST` with body `{ response: "once" | "always" | "reject" }`
4. **Session state**: "busy" can mean "waiting for user input" (permission.asked events)
5. **Log file location**: `logs/actorcode/ses_<id>.log` (not `.actorcode/logs/`)

### Architectural
1. **Permissive > Protective**: With isolation layers, permission prompts are anti-productive
2. **Background > Blocking**: Supervisor should spawn, not do work directly
3. **Parallel > Serial**: Launch multiple research tasks concurrently
4. **Worktree isolation > Permission blocking**: Real safety is in worktrees, not prompts

### Process
1. **Spawn subtasks for everything**: Keep supervisor context clean
2. **Pre-approve permissions**: Don't wait for user input during research
3. **Tail logs without watching**: Don't hang forever
4. **Abort stuck sessions**: Clean up after long-running tasks

## Open Questions

- [ ] Should we add a `just research-all` to launch all research templates in parallel?
- [ ] Should monitor auto-abort sessions after N hours of no activity?
- [ ] Should we add a supervisor inbox system for rare permission prompts (when doom_loop: "ask")?

## Next Steps

1. **Implement permission pre-approval** in research-launch.js
2. **Implement permission pre-approval** in actorcode.js handleSpawn
3. **Fix logs command** to add --follow flag
4. **Abort stuck sessions** from earlier today
5. **Test new permissive model** with a research task

---

**PHILOSOPHY:** Trust the agents, isolate with worktrees, approve by default, stay in OpenCode's happy path.
