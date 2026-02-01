#!/usr/bin/env node
import { runResearchTasks } from "./lib/research.js";
import { parseArgs } from "./lib/args.js";
import { logSupervisor } from "./lib/logs.js";

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
Report each finding immediately with [LEARNING] SECURITY: description`
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
Report each finding immediately with [LEARNING] REFACTOR: description`
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
Report each finding immediately with [LEARNING] DOCS: description`
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
Report each finding immediately with [LEARNING] PERFORMANCE: description`
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
Report each finding immediately with [LEARNING] BUG: description`
  }
};

async function main() {
  const { args, options } = parseArgs(process.argv.slice(2));
  const command = args[0];

  if (!command || options.help) {
    console.log("Usage: research-runner <template> [template2...] [--parallel] [--supervisor <session_id>]");
    console.log("\nAvailable templates:");
    Object.keys(RESEARCH_TEMPLATES).forEach((name) => {
      console.log(`  ${name}`);
    });
    return;
  }

  const templates = args.filter((arg) => RESEARCH_TEMPLATES[arg]);
  
  if (templates.length === 0) {
    console.error("No valid templates specified");
    process.exit(1);
  }

  const tasks = templates.map((name) => ({
    ...RESEARCH_TEMPLATES[name],
    title: `${RESEARCH_TEMPLATES[name].title} (${new Date().toISOString()})`
  }));

  console.log(`Starting ${tasks.length} research task(s): ${templates.join(", ")}`);

  const results = await runResearchTasks(tasks, {
    supervisorSessionId: options.supervisor,
    onLearning: (learning) => {
      console.log(`\n[${learning.category}] ${learning.description.slice(0, 100)}...`);
      console.log(`  Session: ${learning.sessionId}`);
    },
    onComplete: (completion) => {
      console.log(`\n[COMPLETE] Session ${completion.sessionId} finished`);
    },
    onError: (error) => {
      console.error(`\n[ERROR] ${error.message}`);
    }
  });

  console.log("\n=== Research Summary ===");
  console.log(`Learnings: ${results.learnings.length}`);
  console.log(`Completed: ${results.completed.length}`);
  console.log(`Errors: ${results.errors.length}`);

  if (results.learnings.length > 0) {
    console.log("\n=== Learnings by Category ===");
    const byCategory = {};
    results.learnings.forEach((l) => {
      byCategory[l.category] = (byCategory[l.category] || 0) + 1;
    });
    Object.entries(byCategory).forEach(([cat, count]) => {
      console.log(`  ${cat}: ${count}`);
    });
  }

  await logSupervisor(`research complete tasks=${tasks.length} learnings=${results.learnings.length}`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
