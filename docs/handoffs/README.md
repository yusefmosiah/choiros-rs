# Handoffs

Session handoff documents for the choirOS project.

## Quick Start

Create a new handoff:
```bash
cd /Users/wiz/choiros-rs
python skills/session-handoff/scripts/create_handoff.py "task-description"
```

Validate a handoff before finalizing:
```bash
python skills/session-handoff/scripts/validate_handoff.py docs/handoffs/YYYY-MM-DD-HHMMSS-task-description.md
```

List all handoffs:
```bash
python skills/session-handoff/scripts/list_handoffs.py
```

Check if a handoff is still current:
```bash
python skills/session-handoff/scripts/check_staleness.py docs/handoffs/YYYY-MM-DD-HHMMSS-task-description.md
```

## Directory Structure

```
docs/handoffs/        # Handoff storage (this directory)
├── archive/          # Archived/old handoffs
└── *.md              # Active handoff documents

skills/session-handoff/  # Skill implementation
├── SKILL.md           # Full skill documentation
└── scripts/           # Handoff management scripts
    ├── create_handoff.py
    ├── validate_handoff.py
    ├── list_handoffs.py
    └── check_staleness.py
```

## What Are Handoffs?

Handoffs are comprehensive documents that capture:
- Current work state and context
- Codebase architecture insights
- Decisions made and their rationale
- Immediate next steps for the next agent
- Blockers, assumptions, and potential gotchas

They enable seamless continuation of work across AI agent sessions.

## Creating a Handoff

1. **Generate scaffold:**
   ```bash
   python skills/session-handoff/scripts/create_handoff.py "implementing-feature-x"
   ```

2. **Fill in the template:**
   - Open the generated `.md` file
   - Replace all `[TODO: ...]` placeholders
   - Focus especially on **Current State Summary** and **Immediate Next Steps**
   - Document important context, decisions, and gotchas

3. **Validate:**
   ```bash
   python skills/session-handoff/scripts/validate_handoff.py <handoff-file>
   ```
   - Checks for TODOs, completeness, secrets, file references
   - Must score 70+ with no secrets detected

4. **Commit:**
   ```bash
   git add docs/handoffs/YYYY-MM-DD-HHMMSS-*.md
   git commit -m "docs: handoff for feature X"
   ```

## Handoff Naming Convention

Format: `YYYY-MM-DD-HHMMSS-{slug}.md`

Examples:
- `2026-01-31-143022-implementing-auth.md`
- `2026-01-31-220519-baml-chat-agent-implementation.md`

## Chaining Handoffs

For long-running work, link handoffs together:

```bash
# Create second handoff referencing the first
python skills/session-handoff/scripts/create_handoff.py "auth-part-2" --continues-from 2026-01-31-143022-implementing-auth.md
```

Each handoff should reference its predecessor in the "Handoff Chain" section.

## Archive

Old/superseded handoffs go in `archive/`:
- Handoffs older than 30 days
- Superseded by newer handoffs
- Completed projects

## Security

**Never commit secrets in handoffs:**
- List env var names only (e.g., `AWS_BEARER_TOKEN_BEDROCK`)
- Never include actual values, API keys, or tokens
- Run `validate_handoff.py` before committing to check for secrets

## Differences from Default Setup

This project uses `docs/handoffs/` instead of `.claude/handoffs/` because:
1. Handoffs are valuable documentation and belong in version control
2. Avoids vendor-specific naming (".claude") in the codebase
3. Located in `docs/` for discoverability

The scripts in `scripts/` are modified versions of the default handoff scripts, updated to use `docs/handoffs/` as the storage location.

## Tips for Good Handoffs

1. **Start with the most important info:** Current State, Important Context, Immediate Next Steps
2. **Be specific:** "Fix the bug in auth" is bad; "The auth middleware fails on JWT expiration - see auth/middleware.rs:45" is good
3. **Document decisions:** Why did you choose approach A over B?
4. **List gotchas:** What might trip up the next agent?
5. **Keep it factual:** Avoid speculation; document what IS, not what might be
6. **Commit your work:** Handoffs should reference committed code, not uncommitted changes

## Examples

Here are example handoffs to show the expected structure:

### Example: 2026-01-31-220519-baml-chat-agent-implementation.md

**Template structure:**

```markdown
# Handoff: [Title]

## Session Metadata
- Created: YYYY-MM-DD HH:MM:SS
- Project: /path/to/project
- Branch: main
- Session duration: ~X hours

### Recent Commits (for context)
  - 1a2b3c4 feat: commit message
  - 5d6e7f8 fix: commit message

## Handoff Chain
- **Continues from**: [Previous-handoff.md](./previous-handoff.md)
  - Previous title: Title of previous handoff
- **Supersedes**: None

## Current State Summary
[Bullet points of what's done and NOT done]

## Codebase Understanding
### Architecture Overview
[High-level architecture description]

### Critical Files
| File | Purpose | Relevance |

### Key Patterns Discovered
1. Pattern description
2. Pattern description

## Work Completed
### Tasks Finished
- [x] Task 1
- [x] Task 2

### Files Modified/Created
| File | Changes | Rationale |

### Decisions Made
| Decision | Options Considered | Rationale |

## Pending Work
### Immediate Next Steps
1. **Task 1** (CRITICAL)
   - Subtask 1
   - Subtask 2

### Blockers/Open Questions
- [ ] Blocker description

### Deferred Items
- Deferred task description

## Context for Resuming Agent
### Important Context
**1. Context 1:**
- Detail about context 1

**2. Context 2:**
- Detail about context 2

### Assumptions Made
- Assumption description

### Potential Gotchas
- Gotcha description

## Environment State
### Tools/Services Used
- Tool/service description

### Active Processes
- Process description

### Environment Variables (Names Only)
- VAR_NAME

## Related Resources
- Reference link

---

**Security Reminder**: No secrets in this handoff. All API keys are in `.env` file (not committed).

**DevX Note**: Additional context about development experience
```

**See actual examples in directory:**
- `2026-01-31-220519-baml-chat-agent-implementation.md` - Feature implementation
- `2026-02-01-020951-choir-chat-testing-phase1.md` - Testing phase
- `2026-01-31-tests-complete.md` - Test completion

## See Also

- Global skill reference: `~/.config/opencode/skills/session-handoff/`
- Template: `~/.config/opencode/skills/session-handoff/references/handoff-template.md`
