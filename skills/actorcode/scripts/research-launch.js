#!/usr/bin/env node
import { spawn } from "child_process";
import { parseArgs } from "./lib/args.js";
import { createClient } from "./lib/client.js";
import { logSupervisor } from "./lib/logs.js";
import { updateSessionRegistry } from "./lib/registry.js";

const DIRECTORY = process.cwd();

const RESEARCH_TEMPLATES = {
  "security-audit": {
    title: "Security audit",
    agent: "explore",
    tier: "micro",
    prompt: `Audit the codebase for security vulnerabilities. Focus on:
- Hardcoded secrets, API keys, passwords
- Path traversal vulnerabilities  
- SQL injection risks
- Authentication/authorization gaps
- CORS misconfigurations
- Input validation issues
- Unsafe file operations

Report each finding immediately with [LEARNING] SECURITY: description
Continue working after each report. Mark completion with [COMPLETE]`
  },
  
  "code-quality": {
    title: "Code quality review",
    agent: "explore",
    tier: "nano",
    prompt: `Review code quality across the codebase. Focus on:
- Code smells and anti-patterns
- Unused imports and dead code
- Overly complex functions
- Missing error handling
- Inconsistent naming conventions
- Missing documentation

Report each finding immediately with [LEARNING] REFACTOR: description
Continue working after each report. Mark completion with [COMPLETE]`
  },
  
  "docs-gap": {
    title: "Documentation gap analysis",
    agent: "explore",
    tier: "pico",
    prompt: `Analyze documentation coverage. Focus on:
- Missing README files
- Undocumented public APIs
- Outdated docs
- Missing examples
- Unclear setup instructions

Report each finding immediately with [LEARNING] DOCS: description
Continue working after each report. Mark completion with [COMPLETE]`
  },
  
  "performance": {
    title: "Performance analysis",
    agent: "explore",
    tier: "nano",
    prompt: `Analyze performance bottlenecks. Focus on:
- Inefficient database queries
- Unnecessary clones/allocations
- Blocking operations in async code
- Missing caching opportunities
- Large dependencies

Report each finding immediately with [LEARNING] PERFORMANCE: description
Continue working after each report. Mark completion with [COMPLETE]`
  },
  
  "bug-hunt": {
    title: "Bug hunt",
    agent: "explore",
    tier: "micro",
    prompt: `Hunt for bugs in the codebase. Focus on:
- Race conditions
- Off-by-one errors
- Null/None handling
- Resource leaks
- Logic errors
- Unhandled edge cases

Report each finding immediately with [LEARNING] BUG: description
Continue working after each report. Mark completion with [COMPLETE]`
  },
  
  "concurrent-dev-envs": {
    title: "Concurrent development environments research",
    agent: "explore",
    tier: "micro",
    prompt: `Research best practices for concurrent development environments where multiple AI agents work simultaneously. Focus on:
- Containerized remote dev environments (Docker, Kubernetes, etc.)
- Git worktree workflows and limitations
- Nix for reproducible dev environments
- Dev container standards (VS Code dev containers, etc.)
- Cloud-based IDEs (GitHub Codespaces, Gitpod, etc.)
- File locking and conflict resolution strategies
- Resource allocation for multiple concurrent environments
- Companies/projects doing this well (examples, case studies)
- Trade-offs: local vs remote, isolation vs performance

Report each finding immediately with [LEARNING] ARCHITECTURE: description
Continue working after each report. Mark completion with [COMPLETE]`
  },
  
  "nix-devops": {
    title: "Nix for development and production",
    agent: "explore",
    tier: "micro",
    prompt: `Research using Nix for development environments and production builds. Focus on:
- Nix flakes for reproducible builds
- Nix dev shells and direnv integration
- How many concurrent dev shells can run on a c5.large
- Nix for container image building (nix2container, etc.)
- NixOS for production servers
- Cachix for binary caching
- Nix vs Docker for dev environments
- Real-world examples of Nix in production
- Resource overhead of Nix

Report each finding immediately with [LEARNING] ARCHITECTURE: description
Continue working after each report. Mark completion with [COMPLETE]`
  },
  
  "tailscale-remote-dev": {
    title: "Tailscale and Termius for remote development",
    agent: "explore",
    tier: "nano",
    prompt: `Research Tailscale and Termius for remote development setups. Focus on:
- Tailscale setup for secure remote access to dev servers
- Tailscale SSH vs traditional SSH
- Termius (iOS terminal app) for mobile remote access
- Combining Tailscale with containerized dev environments
- Security best practices
- Performance considerations
- Cost analysis
- Alternatives (ZeroTier, WireGuard, etc.)
- Setting up on AWS EC2 (c5.large or similar)

Report each finding immediately with [LEARNING] ARCHITECTURE: description
Continue working after each report. Mark completion with [COMPLETE]`
  },
  
  "pico-monitor-pattern": {
    title: "Pico agent monitoring and handoff patterns",
    agent: "explore",
    tier: "pico",
    prompt: `Research patterns for using lightweight AI agents (pico tier) as live monitors with periodic handoffs. Focus on:
- How to detect when an agent's context window is filling up
- Strategies for handing off monitoring tasks to fresh agents
- Summarizing and chunking continuous data streams
- Examples of successful monitoring agent architectures
- Best practices for stateless vs stateful monitoring
- How to maintain continuity across agent respawns
- Tools and frameworks for agent orchestration
- Cost optimization for long-running monitoring tasks

Report each finding immediately with [LEARNING] ARCHITECTURE: description
Continue working after each report. Mark completion with [COMPLETE]`
  }
};

function buildPromptWithReporting({ prompt, title, supervisorSessionId }) {
  return [
    "You are a research subagent working incrementally.",
    "",
    "CRITICAL: Report learnings as you discover them.",
    "",
    "Reporting protocol:",
    "1. When you find something important, IMMEDIATELY include it in your response",
    "2. Format: [LEARNING] <category>: <brief description>",
    "3. Categories: BUG, SECURITY, DOCS, REFACTOR, PERFORMANCE, ARCHITECTURE",
    "4. Continue working after reporting—don't wait for response",
    "5. Mark task completion with [COMPLETE] at the end",
    "",
    "Example:",
    "[LEARNING] SECURITY: Hardcoded API key found in src/config.rs line 45",
    "[LEARNING] BUG: Race condition in actor initialization, details in logs",
    "...",
    "[COMPLETE]",
    "",
    "Task:",
    prompt
  ].join("\n");
}

async function main() {
  const { args, options } = parseArgs(process.argv.slice(2));
  const templates = args.filter((arg) => RESEARCH_TEMPLATES[arg]);
  
  if (templates.length === 0) {
    console.log("Usage: research-launch <template> [template2...] [--monitor]");
    console.log("\nAvailable templates:");
    Object.keys(RESEARCH_TEMPLATES).forEach((name) => {
      console.log(`  ${name} - ${RESEARCH_TEMPLATES[name].title}`);
    });
    return;
  }

  const client = createClient();
  const sessionIds = [];

  for (const templateName of templates) {
    const template = RESEARCH_TEMPLATES[templateName];
    const title = `${template.title} (${new Date().toISOString()})`;
    
    const sessionResponse = await client.session.create({
      query: { directory: DIRECTORY },
      body: { title }
    });
    
    const sessionId = sessionResponse.data?.id;
    if (!sessionId) {
      console.error(`Failed to create session for ${templateName}`);
      continue;
    }

    sessionIds.push({ template: templateName, sessionId, title });

    await updateSessionRegistry(sessionId, {
      title,
      agent: template.agent,
      tier: template.tier,
      status: "spawned",
      createdAt: Date.now()
    });

    const fullPrompt = buildPromptWithReporting({
      prompt: template.prompt,
      title,
      supervisorSessionId: options.supervisor
    });

    // Map tier to model
    const MODEL_TIERS = {
      pico: "zai-coding-plan/glm-4.7-flash",
      nano: "zai-coding-plan/glm-4.7",
      micro: "kimi-for-coding/k2p5",
      milli: "openai/gpt-5.2-codex"
    };
    const model = MODEL_TIERS[template.tier];
    const [providerID, modelID] = model.split("/");

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
          doom_loop: "ask"
        }
      }
    });

    await logSupervisor(`research-launched template=${templateName} session=${sessionId}`);
  }

  console.log("\n=== Launched Research Tasks ===");
  sessionIds.forEach(({ template, sessionId, title }) => {
    console.log(`${template}: ${sessionId}`);
    console.log(`  Title: ${title}`);
    console.log(`  Monitor: just actorcode logs --id ${sessionId}`);
    console.log(`  Messages: just actorcode messages --id ${sessionId} --role assistant --latest --wait`);
    console.log();
  });

  if (options.monitor) {
    console.log("Starting monitor...");
    const monitor = spawn("node", [
      "skills/actorcode/scripts/research-monitor.js",
      ...sessionIds.map((s) => s.sessionId)
    ], {
      detached: true,
      stdio: "ignore"
    });
    monitor.unref();
    console.log(`Monitor PID: ${monitor.pid}`);
  }

  console.log("\nTo check progress:");
  console.log(`just actorcode messages --id <session_id> --role assistant --latest --wait`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
