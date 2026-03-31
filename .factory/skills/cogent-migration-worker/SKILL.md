# Deprecated Mission-Specific Skill

This skill is not used by the current choiros-rs hard-cutover mission.

Current mission guidance:
- do **not** change `~/cogent`
- do **not** add `.cagent` fallback support there
- perform a repo-local hard cutover inside `choiros-rs` only

Workers should use `rename-worker` for the active mission.
