#!/usr/bin/env node
/**
 * Finding Fix Orchestrator
 * 
 * Spawns isolated actorcode agents in git worktrees to fix findings.
 * Handles dependency ordering and safe branch merging.
 * 
 * Architecture:
 * 1. Parse findings and build dependency graph
 * 2. Create git worktrees for isolated development
 * 3. Spawn agents in worktrees with specific fixes
 * 4. Run tests in worktrees
 * 5. Merge branches safely with dependency awareness
 */

import { spawn } from "child_process";
import fs from "fs/promises";
import path from "path";
import { loadFindings } from "./lib/findings.js";

const WORKTREE_BASE = ".actorcode/worktrees";
const BATCH_SIZE = 3; // Max concurrent fixes

class FindingFixOrchestrator {
  constructor() {
    this.worktrees = new Map(); // findingId -> worktreePath
    this.branches = new Map(); // findingId -> branchName
    this.results = new Map(); // findingId -> result
  }

  /**
   * Build dependency graph from findings
   * Dependencies based on:
   * - File overlap (can't edit same file concurrently)
   * - Category ordering (DOCS before REFACTOR, etc.)
   * - Directory structure (parent before child)
   */
  async buildDependencyGraph(findings) {
    const graph = new Map();
    const fileToFindings = new Map();
    
    // Build file -> findings map for efficient lookup
    for (const finding of findings) {
      const fileMatches = finding.description.match(/[\w\-./]+\.(rs|md|toml|js|py)/g) || [];
      for (const file of fileMatches) {
        if (!fileToFindings.has(file)) {
          fileToFindings.set(file, []);
        }
        fileToFindings.get(file).push(finding);
      }
    }
    
    for (const finding of findings) {
      const deps = [];
      
      // File overlap: create one-way dependency based on timestamp
      // Earlier finding depends on later finding (later will fix first)
      const fileMatches = finding.description.match(/[\w\-./]+\.(rs|md|toml|js|py)/g) || [];
      for (const file of fileMatches) {
        const related = fileToFindings.get(file) || [];
        for (const other of related) {
          if (other.id === finding.id) continue;
          
          // One-way: earlier depends on later
          if (new Date(finding.timestamp) < new Date(other.timestamp)) {
            if (!deps.includes(other.id)) {
              deps.push(other.id);
            }
          }
        }
      }
      
      // Category ordering: DOCS < REFACTOR < BUG < SECURITY
      const priority = { DOCS: 1, REFACTOR: 2, BUG: 3, SECURITY: 4, PERFORMANCE: 5 };
      const findingPriority = priority[finding.category] || 99;
      
      for (const other of findings) {
        if (other.id === finding.id) continue;
        const otherPriority = priority[other.category] || 99;
        
        // Lower priority must come first
        if (findingPriority > otherPriority && !deps.includes(other.id)) {
          deps.push(other.id);
        }
      }
      
      graph.set(finding.id, deps);
    }
    
    return graph;
  }

  /**
   * Topological sort for dependency ordering
   */
  sortByDependencies(graph, findings) {
    const visited = new Set();
    const temp = new Set();
    const sorted = [];
    
    const visit = (id) => {
      if (temp.has(id)) throw new Error(`Circular dependency detected: ${id}`);
      if (visited.has(id)) return;
      
      temp.add(id);
      const deps = graph.get(id) || [];
      for (const dep of deps) {
        visit(dep);
      }
      temp.delete(id);
      visited.add(id);
      sorted.push(id);
    };
    
    for (const finding of findings) {
      visit(finding.id);
    }
    
    return sorted.map(id => findings.find(f => f.id === id));
  }

  /**
    * Create git worktree for isolated development
    */
  async createWorktree(finding) {
    const branchName = `fix/${finding.category.toLowerCase()}/${finding.id.slice(-8)}`;
    const worktreePath = path.join(process.cwd(), WORKTREE_BASE, finding.id);
    
    console.log(`Creating worktree for ${finding.id.slice(-8)}...`);
    
    // Check if branch already exists
    try {
      const branchExists = await this.execGit(["branch", "--list", branchName]);
      if (branchExists.trim()) {
        console.log(`  Branch ${branchName} already exists, deleting...`);
        try {
          await this.execGit(["branch", "-D", branchName]);
        } catch (e) {
          // Branch might be used by worktree, try to remove worktree first
          console.log(`  Removing existing worktree...`);
          try {
            await this.execGit(["worktree", "remove", worktreePath]);
            await this.execGit(["branch", "-D", branchName]);
          } catch (e2) {
            console.error(`  Warning: Could not remove existing branch/worktree: ${e2.message}`);
          }
        }
      }
    } catch (e) {
      // Continue
    }
    
    // Check if worktree directory exists
    try {
      await fs.access(worktreePath);
      console.log(`  Worktree directory exists, removing...`);
      await fs.rm(worktreePath, { recursive: true, force: true });
    } catch (e) {
      // Directory doesn't exist, that's fine
    }
    
    // Ensure on main branch first
    await this.execGit(["checkout", "main"]);
    
    // Create branch from main
    await this.execGit(["checkout", "-b", branchName, "main"]);
    
    // Create worktree
    await this.execGit(["worktree", "add", worktreePath, branchName]);
    
    this.worktrees.set(finding.id, worktreePath);
    this.branches.set(finding.id, branchName);
    
    return { worktreePath, branchName };
  }

  /**
    * Determine appropriate model tier for a finding
    */
  selectTier(finding) {
    const desc = finding.description.toLowerCase();
    
    // DOCS findings: pico for simple docs, nano for API docs
    if (finding.category === "DOCS") {
      if (desc.includes("api") || desc.includes("component") || desc.includes("actor")) {
        return "nano";
      }
      return "pico";
    }
    
    // TEST findings: nano for writing tests
    if (finding.category === "TEST") {
      return "nano";
    }
    
    // Other categories: micro as default
    return "micro";
  }

  /**
    * Spawn agent in worktree to fix finding
    */
  async spawnFixAgent(finding, worktreePath) {
    const tier = this.selectTier(finding);
    const prompt = this.buildFixPrompt(finding);
    const title = `Fix: ${finding.category} - ${finding.id.slice(-8)}`;
    
    console.log(`Spawning agent for ${finding.id.slice(-8)} in ${worktreePath} (tier: ${tier})...`);
    
    // Spawn actorcode agent in worktree
    const child = spawn("node", [
      "skills/actorcode/scripts/actorcode.js",
      "spawn",
      "--title", title,
      "--agent", "general",
      "--tier", tier,
      "--prompt", prompt
    ], {
      cwd: worktreePath,
      stdio: "inherit"
    });
    
    return new Promise((resolve, reject) => {
      child.on("exit", (code) => {
        if (code === 0) {
          resolve({ success: true, findingId: finding.id });
        } else {
          reject(new Error(`Agent failed with code ${code}`));
        }
      });
    });
  }

  /**
   * Build prompt for fixing a finding
   */
  buildFixPrompt(finding) {
    return [
      `Fix this ${finding.category} finding:`,
      "",
      finding.description,
      "",
      "Requirements:",
      "1. Make minimal, focused changes",
      "2. Follow existing code conventions",
      "3. Run tests to verify: cargo test -p sandbox",
      "4. Update progress.md if needed",
      "5. Mark [COMPLETE] when done",
      "",
      "Work in isolation - this is a git worktree.",
      "Your changes will be merged back to main after review."
    ].join("\n");
  }

  /**
   * Run tests in worktree
   */
  async runTests(worktreePath) {
    console.log(`Running tests in ${worktreePath}...`);
    
    return new Promise((resolve, reject) => {
      const child = spawn("cargo", ["test", "-p", "sandbox", "--lib"], {
        cwd: worktreePath,
        stdio: "pipe"
      });
      
      let output = "";
      child.stdout.on("data", (data) => { output += data; });
      child.stderr.on("data", (data) => { output += data; });
      
      child.on("exit", (code) => {
        resolve({
          success: code === 0,
          output,
          code
        });
      });
    });
  }

  /**
   * Merge branch back to main safely
   */
  async mergeBranch(findingId) {
    const branchName = this.branches.get(findingId);
    if (!branchName) throw new Error(`No branch for ${findingId}`);
    
    console.log(`Merging ${branchName} to main...`);
    
    // Checkout main
    await this.execGit(["checkout", "main"]);
    
    // Merge with --no-ff to preserve history
    await this.execGit(["merge", "--no-ff", "-m", `fix: ${findingId.slice(-8)}`, branchName]);
    
    // Remove worktree
    const worktreePath = this.worktrees.get(findingId);
    if (worktreePath) {
      await this.execGit(["worktree", "remove", worktreePath]);
      this.worktrees.delete(findingId);
    }
    
    // Delete branch
    await this.execGit(["branch", "-d", branchName]);
    this.branches.delete(findingId);
    
    return { success: true };
  }

  /**
   * Execute git command
   */
  async execGit(args) {
    return new Promise((resolve, reject) => {
      const child = spawn("git", args, {
        cwd: process.cwd(),
        stdio: "pipe"
      });
      
      let stdout = "";
      let stderr = "";
      
      child.stdout.on("data", (data) => { stdout += data; });
      child.stderr.on("data", (data) => { stderr += data; });
      
      child.on("exit", (code) => {
        if (code === 0) {
          resolve(stdout);
        } else {
          reject(new Error(`git ${args.join(" ")} failed: ${stderr}`));
        }
      });
    });
  }

  /**
   * Main orchestration loop
   */
  async run(options = {}) {
    const { category, limit = 10, dryRun = false } = options;
    
    console.log("=== Finding Fix Orchestrator ===\n");
    
    // Load findings
    const findings = await loadFindings({ category, limit });
    console.log(`Loaded ${findings.length} findings`);
    
    if (findings.length === 0) {
      console.log("No findings to fix.");
      return;
    }
    
    // Build dependency graph
    console.log("\nBuilding dependency graph...");
    const graph = await this.buildDependencyGraph(findings);
    
    // Sort by dependencies
    const sorted = this.sortByDependencies(graph, findings);
    console.log(`Ordered ${sorted.length} findings by dependencies`);
    
    // Show order
    console.log("\nFix order:");
    sorted.forEach((f, i) => {
      const deps = graph.get(f.id) || [];
      console.log(`  ${i + 1}. [${f.category}] ${f.id.slice(-8)}${deps.length > 0 ? ` (depends on: ${deps.map(d => d.slice(-8)).join(", ")})` : ""}`);
    });
    
    if (dryRun) {
      console.log("\n(Dry run - no changes made)");
      return;
    }
    
    // Process in batches respecting dependencies
    console.log("\n=== Processing Fixes ===\n");
    
    const completed = new Set();
    const inProgress = new Set();
    const attempts = new Map(); // findingId -> attempt count
    const MAX_ATTEMPTS = 3;
    
    while (completed.size < sorted.length) {
      // Find ready findings (all deps completed and not maxed out on attempts)
      const ready = sorted.filter(f => {
        if (completed.has(f.id) || inProgress.has(f.id)) return false;
        const att = attempts.get(f.id) || 0;
        if (att >= MAX_ATTEMPTS) return false;
        const deps = graph.get(f.id) || [];
        return deps.every(d => completed.has(d));
      }).slice(0, BATCH_SIZE - inProgress.size);
      
      if (ready.length === 0 && inProgress.size === 0) {
        // No ready findings and nothing in progress - check if we have failed ones
        const failed = sorted.filter(f => {
          const att = attempts.get(f.id) || 0;
          return att >= MAX_ATTEMPTS && !completed.has(f.id);
        });
        
        if (failed.length > 0) {
          console.log("\n=== Failed Fixes (max attempts reached) ===");
          failed.forEach(f => {
            const result = this.results.get(f.id);
            console.log(`  ✗ ${f.id.slice(-8)}: ${result?.error || "Unknown error"}`);
          });
          break;
        } else {
          throw new Error("Deadlock: no ready findings but not all completed");
        }
      }
      
      // Spawn agents for ready findings
      const promises = ready.map(async (finding) => {
        inProgress.add(finding.id);
        const attempt = (attempts.get(finding.id) || 0) + 1;
        attempts.set(finding.id, attempt);
        
        try {
          // Create worktree
          const { worktreePath } = await this.createWorktree(finding);
          
          // Spawn agent
          await this.spawnFixAgent(finding, worktreePath);
          
          // Run tests
          const testResult = await this.runTests(worktreePath);
          
          if (!testResult.success) {
            console.error(`Tests failed for ${finding.id.slice(-8)}:`);
            console.error(testResult.output.slice(-500));
            return { findingId: finding.id, success: false, error: "Tests failed" };
          }
          
          // Merge
          await this.mergeBranch(finding.id);
          
          completed.add(finding.id);
          inProgress.delete(finding.id);
          
          return { findingId: finding.id, success: true };
        } catch (error) {
          inProgress.delete(finding.id);
          return { findingId: finding.id, success: false, error: error.message };
        }
      });
      
      if (promises.length > 0) {
        const results = await Promise.all(promises);
        results.forEach(r => {
          this.results.set(r.findingId, r);
          const status = r.success ? "✓" : "✗";
          const att = attempts.get(r.findingId) || 1;
          console.log(`${status} ${r.findingId.slice(-8)} (attempt ${att}/${MAX_ATTEMPTS})${r.error ? `: ${r.error}` : ""}`);
        });
      } else {
        // Wait for in-progress to complete
        await new Promise(r => setTimeout(r, 1000));
      }
    }
    
    // Summary
    console.log("\n=== Summary ===");
    const successful = Array.from(this.results.values()).filter(r => r.success).length;
    const failed = Array.from(this.results.values()).filter(r => !r.success).length;
    console.log(`Fixed: ${successful}/${sorted.length}`);
    console.log(`Failed: ${failed}/${sorted.length}`);
    
    if (failed > 0) {
      console.log("\nFailed fixes:");
      for (const [id, result] of this.results) {
        if (!result.success) {
          console.log(`  ${id.slice(-8)}: ${result.error}`);
        }
      }
    }
  }
}

// CLI
async function main() {
  const args = process.argv.slice(2);
  const options = {
    category: args.find(a => !a.startsWith("--")),
    limit: parseInt(args.find(a => a.startsWith("--limit="))?.split("=")[1]) || 10,
    dryRun: args.includes("--dry-run")
  };
  
  const orchestrator = new FindingFixOrchestrator();
  await orchestrator.run(options);
}

main().catch(error => {
  console.error(error.message);
  process.exit(1);
});
