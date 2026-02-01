#!/usr/bin/env node
/**
 * Actorcode Diagnostic Tool
 * Tests the research system end-to-end and identifies issues
 */

import { createClient } from "./lib/client.js";
import { loadRegistry, saveRegistry } from "./lib/registry.js";
import { loadFindings, getStats, appendFinding } from "./lib/findings.js";
import { logSupervisor } from "./lib/logs.js";

const DIRECTORY = process.cwd();

class DiagnosticRunner {
  constructor() {
    this.client = createClient();
    this.results = [];
    this.passed = 0;
    this.failed = 0;
  }

  async test(name, fn) {
    process.stdout.write(`Testing ${name}... `);
    try {
      await fn();
      console.log("✓ PASS");
      this.passed++;
      this.results.push({ name, status: "PASS" });
    } catch (error) {
      console.log(`✗ FAIL: ${error.message}`);
      this.failed++;
      this.results.push({ name, status: "FAIL", error: error.message });
    }
  }

  async run() {
    console.log("\n=== Actorcode Research System Diagnostics ===\n");

    // Test 1: OpenCode Server Connection
    await this.test("OpenCode Server Connection", async () => {
      const response = await this.client.session.list({ query: { directory: DIRECTORY } });
      if (!response.data) throw new Error("No data returned");
    });

    // Test 2: Registry Read/Write
    await this.test("Registry Read/Write", async () => {
      const registry = await loadRegistry();
      if (!registry) throw new Error("Failed to load registry");
      
      // Test atomic write
      const testKey = `test_${Date.now()}`;
      registry.sessions[testKey] = { test: true, createdAt: Date.now() };
      await saveRegistry(registry);
      
      // Verify
      const reloaded = await loadRegistry();
      if (!reloaded.sessions[testKey]) throw new Error("Registry write failed");
      
      // Cleanup
      delete reloaded.sessions[testKey];
      await saveRegistry(reloaded);
    });

    // Test 3: Findings Database
    await this.test("Findings Database", async () => {
      // Test append
      const finding = await appendFinding({
        sessionId: "test_session",
        category: "TEST",
        description: "Test finding for diagnostics"
      });
      
      if (!finding.id) throw new Error("Finding not assigned ID");
      
      // Test read
      const findings = await loadFindings({ limit: 1 });
      if (findings.length === 0) throw new Error("Could not read findings");
      
      // Test stats
      const stats = await getStats();
      if (typeof stats.totalFindings !== "number") throw new Error("Stats failed");
    });

    // Test 4: Session Creation
    await this.test("Session Creation", async () => {
      const response = await this.client.session.create({
        query: { directory: DIRECTORY },
        body: { title: "diagnostic-test-session" }
      });
      
      if (!response.data?.id) throw new Error("No session ID returned");
      this.testSessionId = response.data.id;
    });

    // Test 5: Prompt Delivery
    await this.test("Prompt Delivery", async () => {
      if (!this.testSessionId) throw new Error("No test session");
      
      await this.client.session.promptAsync({
        path: { id: this.testSessionId },
        query: { directory: DIRECTORY },
        body: {
          parts: [{ type: "text", text: "[LEARNING] TEST: This is a test diagnostic finding\n[COMPLETE]" }]
        }
      });
      
      // Wait a moment for processing
      await new Promise(r => setTimeout(r, 2000));
      
      // Check if message was delivered
      const messagesResponse = await this.client.session.messages({
        path: { id: this.testSessionId },
        query: { directory: DIRECTORY, limit: 5 }
      });
      
      const messages = messagesResponse.data || [];
      if (messages.length === 0) throw new Error("No messages in session");
      
      const userMessages = messages.filter(m => m.info?.role === "user");
      if (userMessages.length === 0) throw new Error("User prompt not found");
    });

    // Test 6: Learning Tag Parsing
    await this.test("Learning Tag Parsing", async () => {
      const testText = `
[LEARNING] SECURITY: Hardcoded API key found
[LEARNING] BUG: Race condition in init
[LEARNING] REFACTOR: Unused import
[COMPLETE]
      `;
      
      const regex = /\[LEARNING\]\s*(\w+):\s*(.+?)(?=\[LEARNING\]|\[COMPLETE\]|$)/gs;
      const matches = [];
      let match;
      
      while ((match = regex.exec(testText)) !== null) {
        matches.push({ category: match[1], description: match[2].trim() });
      }
      
      if (matches.length !== 3) throw new Error(`Expected 3 learnings, got ${matches.length}`);
    });

    // Test 7: Research Session Analysis
    await this.test("Research Session Analysis", async () => {
      const registry = await loadRegistry();
      const sessions = Object.entries(registry.sessions)
        .filter(([id]) => id.startsWith("ses_"));
      
      console.log(`\n    Found ${sessions.length} sessions in registry`);
      
      // Check for orphaned sessions (no recent activity)
      const now = Date.now();
      const oneHour = 60 * 60 * 1000;
      const orphaned = sessions.filter(([id, data]) => {
        const lastActivity = data.lastEventAt || data.createdAt;
        return lastActivity && (now - lastActivity) > oneHour;
      });
      
      if (orphaned.length > 10) {
        console.log(`\n    ⚠️  Warning: ${orphaned.length} sessions with no activity >1 hour`);
      }
      
      // Check research sessions specifically
      const researchSessions = sessions.filter(([id, data]) => {
        return data.title?.includes("audit") || 
               data.title?.includes("review") ||
               data.title?.includes("Security") ||
               data.title?.includes("Code quality");
      });
      
      console.log(`    Found ${researchSessions.length} research sessions`);
      
      for (const [id, data] of researchSessions.slice(0, 3)) {
        console.log(`\n    Session: ${id.slice(-8)}`);
        console.log(`      Title: ${data.title?.substring(0, 50)}...`);
        console.log(`      Status: ${data.status}`);
        console.log(`      Created: ${data.createdAt ? new Date(data.createdAt).toISOString() : "unknown"}`);
        
        // Try to get messages
        try {
          const response = await this.client.session.messages({
            path: { id },
            query: { directory: DIRECTORY, limit: 5 }
          });
          const messages = response.data || [];
          const assistantMsgs = messages.filter(m => m.info?.role === "assistant");
          console.log(`      Assistant messages: ${assistantMsgs.length}`);
          
          if (assistantMsgs.length > 0) {
            const text = assistantMsgs.map(m => 
              m.parts?.filter(p => p.type === "text").map(p => p.text).join("")
            ).join("");
            
            const hasLearning = text.includes("[LEARNING]");
            const hasComplete = text.includes("[COMPLETE]");
            console.log(`      Has [LEARNING]: ${hasLearning}`);
            console.log(`      Has [COMPLETE]: ${hasComplete}`);
          }
        } catch (e) {
          console.log(`      Error checking messages: ${e.message}`);
        }
      }
    });

    // Cleanup test session
    if (this.testSessionId) {
      try {
        await this.client.session.abort({
          path: { id: this.testSessionId },
          query: { directory: DIRECTORY }
        });
      } catch (e) {
        // Ignore cleanup errors
      }
    }

    // Summary
    console.log("\n=== Diagnostic Summary ===");
    console.log(`Passed: ${this.passed}/${this.results.length}`);
    console.log(`Failed: ${this.failed}/${this.results.length}`);
    
    if (this.failed > 0) {
      console.log("\nFailed tests:");
      this.results
        .filter(r => r.status === "FAIL")
        .forEach(r => console.log(`  - ${r.name}: ${r.error}`));
    }
    
    // Recommendations
    console.log("\n=== Recommendations ===");
    if (this.results.find(r => r.name === "Registry Read/Write" && r.status === "FAIL")) {
      console.log("• Registry has race condition - implement file locking");
    }
    if (this.results.find(r => r.name === "Prompt Delivery" && r.status === "FAIL")) {
      console.log("• Prompts not being delivered - check OpenCode server and agent availability");
    }
    
    console.log("\n• Consider implementing session cleanup for old entries");
    console.log("• Add retry logic for registry operations");
    console.log("• Monitor for [LEARNING] tags in real-time with research-monitor");
  }
}

const runner = new DiagnosticRunner();
runner.run().catch(console.error);
