# Handoffs

Session handoff documents for the choirOS project.

## Quick Start

Create a new handoff:
```bash
cd /Users/wiz/choiros-rs
python docs/handoffs/scripts/create_handoff.py "task-description"
```

Validate a handoff before finalizing:
```bash
python docs/handoffs/scripts/validate_handoff.py docs/handoffs/YYYY-MM-DD-HHMMSS-task-description.md
```

List all handoffs:
```bash
python docs/handoffs/scripts/list_handoffs.py
```

Check if a handoff is still current:
```bash
python docs/handoffs/scripts/check_staleness.py docs/handoffs/YYYY-MM-DD-HHMMSS-task-description.md
```

## Directory Structure

```
docs/handoffs/
├── scripts/           # Handoff management scripts
│   ├── create_handoff.py
│   ├── validate_handoff.py
│   ├── list_handoffs.py
│   └── check_staleness.py
├── archive/           # Archived/old handoffs
└── *.md              # Active handoff documents
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
   python docs/handoffs/scripts/create_handoff.py "implementing-feature-x"
   ```

2. **Fill in the template:**
   - Open the generated `.md` file
   - Replace all `[TODO: ...]` placeholders
   - Focus especially on **Current State Summary** and **Immediate Next Steps**
   - Document important context, decisions, and gotchas

3. **Validate:**
   ```bash
   python docs/handoffs/scripts/validate_handoff.py <handoff-file>
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
python docs/handoffs/scripts/create_handoff.py "auth-part-2" --continues-from 2026-01-31-143022-implementing-auth.md
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

## See Also

- Global skill reference: `~/.config/opencode/skills/session-handoff/`
- Template: `~/.config/opencode/skills/session-handoff/references/handoff-template.md`
