#!/usr/bin/env node
/**
 * Research agent with KPIs and self-verification
 * Each research task spawns with explicit success criteria and verifiers
 */

import { spawn } from "child_process";
import { parseArgs } from "./lib/args.js";
import { createClient } from "./lib/client.js";
import { logSupervisor } from "./lib/logs.js";
import { updateSessionRegistry } from "./lib/registry.js";

const DIRECTORY = process.cwd();

// KPI definitions for each research type
const RESEARCH_KPIS = {
  "security-audit": {
    minFindings: 3,
    maxFindings: 20,
    categories: ["SECURITY"],
    timeLimit: 15, // minutes
    verifier: `Verify the security audit is complete by checking:
1. At least 3 specific vulnerabilities identified with file paths
2. Each finding has a clear severity level (Critical/High/Medium/Low)
3. No generic advice without specific code references
4. If fewer than 3 findings, explicitly state "No critical issues found"`
  },
  
  "code-quality": {
    minFindings: 5,
    maxFindings: 30,
    categories: ["REFACTOR", "BUG"],
    timeLimit: 20,
    verifier: `Verify code quality review by checking:
1. Specific files/functions mentioned for each issue
2. Issues are actionable (can be fixed)
3. No subjective opinions without examples
4. Prioritized by impact`
  },
  
  "docs-gap": {
    minFindings: 3,
    maxFindings: 15,
    categories: ["DOCS"],
    timeLimit: 10,
    verifier: `Verify documentation analysis by checking:
1. Specific files/packages missing docs
2. Clear description of what documentation is needed
3. No vague "needs more docs" without specifics`
  },
  
  "performance": {
    minFindings: 3,
    maxFindings: 20,
    categories: ["PERFORMANCE"],
    timeLimit: 20,
    verifier: `Verify performance analysis by checking:
1. Specific bottlenecks identified with metrics or clear reasoning
2. Quantified impact where possible
3. Actionable optimization suggestions`
  },
  
  "bug-hunt": {
    minFindings: 2,
    maxFindings: 20,
    categories: ["BUG"],
    timeLimit: 25,
    verifier: `Verify bug hunt by checking:
1. Actual bugs identified (not just code smells)
2. Clear explanation of the bug and its impact
3. Steps to reproduce or conditions that trigger it`
  },
  
  "concurrent-dev-envs": {
    minFindings: 5,
    maxFindings: 25,
    categories: ["ARCHITECTURE"],
    timeLimit: 30,
    verifier: `Verify research quality by checking:
1. Specific tools/technologies named (not just "containerization")
2. Concrete examples or case studies mentioned
3. Trade-offs discussed (not just benefits)
4. Actionable recommendations`
  },
  
  "nix-devops": {
    minFindings: 5,
    maxFindings: 25,
    categories: ["ARCHITECTURE", "COST"],
    timeLimit: 30,
    verifier: `Verify Nix research by checking:
1. Specific Nix features/tools mentioned
2. Real-world examples or comparisons
3. Resource/cost considerations addressed`
  },
  
  "tailscale-remote-dev": {
    minFindings: 4,
    maxFindings: 20,
    categories: ["ARCHITECTURE", "COST", "SECURITY"],
    timeLimit: 25,
    verifier: `Verify Tailscale research by checking:
1. Setup steps outlined
2. Security considerations addressed
3. Cost breakdown provided
4. Alternatives mentioned`
  },
  
  "logging-architecture": {
    minFindings: 6,
    maxFindings: 30,
    categories: ["ARCHITECTURE"],
    timeLimit: 35,
    verifier: `Verify architecture design by checking:
1. Clear log level definitions with use cases
2. Model tier escalation criteria specified
3. Concrete implementation patterns
4. Examples from real systems`
  },

  "kimi-model-string": {
    minFindings: 1,
    maxFindings: 5,
    categories: ["ARCHITECTURE"],
    timeLimit: 10,
    verifier: `Verify by checking:
1. Exact providerID confirmed (likely "moonshotai")
2. Exact modelID confirmed (likely "kimi-k2.5")
3. Format verified as "providerID/modelID"
4. Working example provided`
  },

  "opencode-codepaths": {
    minFindings: 3,
    maxFindings: 10,
    categories: ["ARCHITECTURE", "BUG"],
    timeLimit: 45,
    verifier: `Verify investigation by checking:
1. Location of @ai-sdk/openai-compatible implementation found
2. Header handling logic identified
3. Difference between TUI and API codepaths explained
4. Root cause of User-Agent not being passed identified
5. Potential fix or workaround suggested`
  }
};

const RESEARCH_TEMPLATES = {
  "security-audit": {
    title: "Security audit",
    agent: "explore",
    tier: "nano",
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
    tier: "nano",
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
    tier: "nano",
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
    tier: "nano",
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

LOGGING GUIDELINES:
- Only log [LEARNING] for surprising insights, architectural decisions, or important discoveries
- Don't log routine operations or obvious facts
- Focus on "what would save future developers time"

Report significant findings with [LEARNING] <category>: description
Mark completion with [COMPLETE]`
  },
  
  "nix-devops": {
    title: "Nix for development and production",
    agent: "explore",
    tier: "nano",
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

LOGGING GUIDELINES:
- Only log [LEARNING] for surprising insights, architectural decisions, or important discoveries
- Don't log routine operations or obvious facts
- Focus on "what would save future developers time"

Report significant findings with [LEARNING] <category>: description
Mark completion with [COMPLETE]`
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

LOGGING GUIDELINES:
- Only log [LEARNING] for surprising insights, architectural decisions, or important discoveries
- Don't log routine operations or obvious facts
- Focus on "what would save future developers time"

Report significant findings with [LEARNING] ARCHITECTURE: description
Mark completion with [COMPLETE]`
  },
  
  "logging-architecture": {
    title: "Hierarchical logging and model escalation architecture",
    agent: "explore",
    tier: "nano",
    prompt: `Design a logging architecture with multiple levels and model tier management. Focus on:

LOG LEVELS:
- DEBUG: Verbose operational details (goes to file, not displayed)
- INFO: Normal operations worth knowing (summary view)
- LEARNING: Surprising insights from experience (highlighted, aggregated)
- ERROR: Problems requiring attention (alerted immediately)
- CRITICAL: System failures (escalate to higher tier model)

MODEL TIER MANAGEMENT:
- When to escalate pico → nano → micro → milli mid-task
- How to spawn higher-tier agents for complex debugging
- Handoff patterns that preserve context across tier changes
- Cost-benefit analysis of tier escalation

MONITORING ARCHITECTURE:
- Pico agents as log filters/routers (not primary researchers)
- Nano agents for pattern detection in log streams
- Micro agents for investigation when patterns detected
- Milli agents for critical system failures only

Examples from production systems:
- How Datadog/NewRelic handle log levels
- Kubernetes event aggregation patterns
- How to avoid alert fatigue while catching real issues

Report significant architectural insights with [LEARNING] ARCHITECTURE: description
Mark completion with [COMPLETE]`
  },
  
  "kimi-model-string": {
    title: "Research correct Kimi API model string",
    agent: "explore",
    tier: "nano",
    prompt: `Research the correct model identifier string for Kimi API.

The user has KIMI_API_KEY in .env file but needs the correct model string format.

Current guess: "moonshotai/kimi-k2.5" (providerID: moonshotai, modelID: kimi-k2.5)

Research:
1. Check if "moonshotai/kimi-k2.5" is the correct format
2. What other model strings work with Kimi API?
3. Check OpenCode documentation for provider mappings
4. Look at working examples in the codebase
5. Verify the exact providerID and modelID format

Report findings with [LEARNING] ARCHITECTURE: description
Mark completion with [COMPLETE] and provide the exact working model string.`
  },

  "opencode-codepaths": {
    title: "OpenCode TUI vs Headless API Codepath Investigation",
    agent: "explore",
    tier: "milli",
    prompt: `Investigate why Kimi For Coding API works via OpenCode TUI but fails via headless API.

## Problem

Kimi For Coding works when connected via TUI (/connect → paste API key), but fails with headless API using model string "kimi-for-coding/k2p5". Error: "Kimi For Coding is currently only available for Coding Agents such as Kimi CLI, Claude Code, Roo Code, Kilo Code, etc."

## Key Evidence

1. TUI connection stores credential in ~/.local/share/opencode/auth.json as "kimi-for-coding"
2. Manual curl with "User-Agent: claude-code/1.0" works
3. OpenCode logs show request to https://api.kimi.com/coding/v1/chat/completions
4. Provider uses @ai-sdk/openai-compatible
5. Headers configured in opencode.json aren't being passed through

## Research Tasks

1. Find @ai-sdk/openai-compatible implementation in OpenCode node_modules
   - Location: ~/.config/opencode/node_modules/@ai-sdk/openai-compatible/
   - Check how it handles headers from config
   - Look for header merging/overriding logic

2. Compare TUI vs API provider initialization
   - How does TUI initialize providers differently?
   - Does TUI add special metadata or headers?
   - Check for hardcoded User-Agent in TUI codepath

3. Search OpenCode source for:
   - "User-Agent" header setting
   - "kimi-for-coding" special handling
   - Provider SDK initialization differences
   - Session vs TUI message sending codepaths

4. Check if connected providers get different treatment
   - Does auth.json type="api" vs oauth matter?
   - Is there provider-specific logic for "kimi-for-coding"?

## Expected Findings

- Location where User-Agent is set in OpenCode
- Why headers from opencode.json aren't passed to SDK
- Difference between TUI provider initialization and API usage
- Working solution for headless API with proper headers

Report all findings with [LEARNING] <category>: <description>
Focus on ARCHITECTURE and BUG categories
Mark completion with [COMPLETE]`
  }
};

function buildPromptWithReporting({ prompt, title, supervisorSessionId, kpis }) {
  return [
    "You are a research subagent working incrementally.",
    "",
    "=== COMPLETION CRITERIA ===",
    `You have ${kpis.timeLimit} minutes to complete this task.`,
    `Target: ${kpis.minFindings}-${kpis.maxFindings} quality findings`,
    `Categories: ${kpis.categories.join(", ")}`,
    "",
    "=== VERIFICATION CHECKLIST ===",
    "Before marking [COMPLETE], verify:",
    kpis.verifier.split('\n').map(line => line.replace(/^\d+\.\s*/, '✓ ')).join('\n'),
    "",
    "=== LOGGING GUIDELINES ===",
    "- DEBUG level: Log your reasoning process, tool calls, observations (goes to session log only)",
    "- LEARNING level: Only for SURPRISING insights, unexpected findings, hard-won experience",
    "",
    "When to use [LEARNING]:",
    "- You tried approach A, it failed, you debugged and found why → log the lesson",
    "- You discovered a non-obvious architectural pattern → log the insight", 
    "- You found conflicting information and resolved it → log the resolution",
    "",
    "When NOT to use [LEARNING]:",
    "- Reading a file and describing what it contains (that's observation, not learning)",
    "- Stating obvious facts or general knowledge",
    "- Routine tool operations",
    "- Framework mentions without insight",
    "",
    "Format: [LEARNING] <category>: <brief description>",
    "Categories: BUG, SECURITY, DOCS, REFACTOR, PERFORMANCE, ARCHITECTURE",
    "",
    "=== TASK ===",
    prompt,
    "",
    "=== SELF-VERIFICATION ===",
    "When you think you're done, run these checks:",
    "1. Count your findings - do you have enough?",
    "2. Check each finding against the verification checklist above",
    "3. If insufficient, continue searching",
    "4. Only mark [COMPLETE] when criteria are met",
    "",
    "If you cannot find enough findings after thorough search, mark [COMPLETE] with note: 'Insufficient findings after thorough search'"
  ].join("\n");
}

async function spawnVerifier(sessionId, templateName, kpis) {
  const client = createClient();
  
  const verifierPrompt = [
    `You are a verification agent. Review the findings from research session ${sessionId}.`,
    "",
    "=== VERIFICATION CRITERIA ===",
    `Minimum findings required: ${kpis.minFindings}`,
    `Maximum findings allowed: ${kpis.maxFindings}`,
    `Expected categories: ${kpis.categories.join(", ")}`,
    "",
    "=== VERIFICATION CHECKLIST ===",
    kpis.verifier,
    "",
    "=== YOUR TASK ===",
    "1. Review all findings from the session",
    "2. Check each finding against the criteria",
    "3. Report: PASS if criteria met, FAIL with specific issues if not",
    "4. If FAIL, suggest what the researcher should do to complete the task"
  ].join("\n");
  
  const verifierSession = await client.session.create({
    query: { directory: DIRECTORY },
    body: { title: `Verifier: ${templateName}` }
  });
  
  const model = "zai-coding-plan/glm-4.7"; // nano tier for verification
  const [providerID, modelID] = model.split("/");
  
  await client.session.promptAsync({
    path: { id: verifierSession.data.id },
    query: { directory: DIRECTORY },
    body: {
      parts: [{ type: "text", text: verifierPrompt }],
      agent: "explore",
      model: { providerID, modelID },
      permission: {
        edit: "allow",
        bash: "allow",
        webfetch: "allow",
        doom_loop: "ask"
      }
    }
  });
  
  return verifierSession.data.id;
}

async function main() {
  const { args, options } = parseArgs(process.argv.slice(2));
  const templates = args.filter((arg) => RESEARCH_TEMPLATES[arg]);
  
  if (templates.length === 0) {
    console.log("Usage: research-launch <template> [template2...] [--monitor] [--verify]");
    console.log("\nAvailable templates:");
    Object.keys(RESEARCH_TEMPLATES).forEach((name) => {
      const kpi = RESEARCH_KPIS[name];
      console.log(`  ${name} - ${RESEARCH_TEMPLATES[name].title}`);
      console.log(`         Target: ${kpi.minFindings}-${kpi.maxFindings} findings, ${kpi.timeLimit}min`);
    });
    return;
  }

  const client = createClient();
  const sessionIds = [];

  for (const templateName of templates) {
    const template = RESEARCH_TEMPLATES[templateName];
    const kpis = RESEARCH_KPIS[templateName];
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

    sessionIds.push({ template: templateName, sessionId, title, kpis });

    await updateSessionRegistry(sessionId, {
      title,
      agent: template.agent,
      tier: template.tier,
      status: "spawned",
      createdAt: Date.now(),
      kpis: {
        minFindings: kpis.minFindings,
        maxFindings: kpis.maxFindings,
        timeLimit: kpis.timeLimit,
        categories: kpis.categories
      }
    });

    const fullPrompt = buildPromptWithReporting({
      prompt: template.prompt,
      title,
      supervisorSessionId: options.supervisor,
      kpis
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

    await logSupervisor(`research-launched template=${templateName} session=${sessionId} kpis=${kpis.minFindings}-${kpis.maxFindings}`);
    
    // Spawn verifier if requested
    if (options.verify) {
      const verifierId = await spawnVerifier(sessionId, templateName, kpis);
      console.log(`  Verifier: ${verifierId}`);
    }
  }

  console.log("\n=== Launched Research Tasks ===");
  sessionIds.forEach(({ template, sessionId, title, kpis }) => {
    console.log(`${template}: ${sessionId}`);
    console.log(`  Title: ${title}`);
    console.log(`  KPIs: ${kpis.minFindings}-${kpis.maxFindings} findings, ${kpis.timeLimit}min`);
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
