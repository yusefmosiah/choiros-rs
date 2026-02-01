export function buildPromptContract({
  prompt,
  title,
  agent,
  model,
  tier,
  directory,
  now = new Date()
}) {
  const lines = [
    "You are a subagent working in the ChoirOS repository.",
    directory ? `Working directory: ${directory}` : null,
    `Date: ${now.toISOString()}`,
    title ? `Session title: ${title}` : null,
    agent ? `Agent hint: ${agent}` : null,
    model ? `Model: ${model}${tier ? ` (${tier})` : ""}` : null,
    "",
    "Contract:",
    "- Focus on the requested task and keep changes minimal.",
    "- Follow repo conventions; call out assumptions or blockers.",
    "- Report summary, file paths touched, and tests run.",
    "",
    "INCREMENTAL REPORTING (CRITICAL):",
    "- Report findings AS YOU DISCOVER THEM, not at the end",
    "- Use format: [LEARNING] <category>: <description>",
    "- Categories: BUG, SECURITY, DOCS, REFACTOR, PERFORMANCE, ARCHITECTURE",
    "- Example: [LEARNING] SECURITY: Hardcoded key in config.rs:45",
    "- Continue working after reporting—don't wait for response",
    "",
    "Task:",
    prompt
  ];

  return lines.filter((line) => line !== null && line !== undefined).join("\n");
}
