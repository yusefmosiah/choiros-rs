#!/usr/bin/env node
import { createClient } from "./lib/client.js";
import { logSupervisor, logSession } from "./lib/logs.js";
import { parseArgs } from "./lib/args.js";
import { appendFinding, getStats } from "./lib/findings.js";
import { loadRegistry, updateSessionRegistry } from "./lib/registry.js";

const DIRECTORY = process.cwd();

// Stuck detection thresholds
const STUCK_THRESHOLDS = {
  noOutputMinutes: 10,      // No new messages for 10 minutes
  maxDurationMinutes: 45,   // Max total session duration
  minLearnings: 1,          // At least 1 learning expected
  stuckCheckInterval: 60000 // Check every minute
};

function extractText(message) {
  const parts = message?.parts || [];
  return parts
    .filter((part) => part?.type === "text" && part?.text)
    .map((part) => part.text)
    .join("\n");
}

function parseLearnings(text) {
  const learnings = [];
  const regex = /\[LEARNING\]\s*(\w+):\s*(.+?)(?=\[LEARNING\]|\[COMPLETE\]|$)/gs;
  let match;
  
  while ((match = regex.exec(text)) !== null) {
    learnings.push({
      category: match[1].toUpperCase(),
      description: match[2].trim()
    });
  }
  
  return learnings;
}

class ResearchMonitor {
  constructor(sessionIds) {
    this.sessionIds = sessionIds;
    this.client = createClient();
    this.lastMessageIds = {};
    this.allLearnings = [];
    this.running = true;
    this.sessionStates = {}; // Track state per session
    this.stuckCheckTimer = null;
  }

  async start() {
    await logSupervisor(`research-monitor start sessions=${this.sessionIds.length}`);
    
    // Initialize session states
    for (const sessionId of this.sessionIds) {
      this.sessionStates[sessionId] = {
        lastActivity: Date.now(),
        learningsCount: 0,
        startTime: Date.now(),
        status: "running",
        lastMessageTime: null
      };
    }
    
    // Start stuck detection
    this.stuckCheckTimer = setInterval(() => this.checkStuckSessions(), STUCK_THRESHOLDS.stuckCheckInterval);
    
    process.on("SIGINT", () => {
      this.running = false;
      if (this.stuckCheckTimer) clearInterval(this.stuckCheckTimer);
      this.printSummary();
      process.exit(0);
    });

    while (this.running) {
      for (const sessionId of this.sessionIds) {
        await this.checkSession(sessionId);
      }
      await this.sleep(3000);
    }
  }

  async checkSession(sessionId) {
    try {
      const response = await this.client.session.messages({
        path: { id: sessionId },
        query: { directory: DIRECTORY, limit: 5 }
      });

      const messages = response.data || [];
      const state = this.sessionStates[sessionId];
      
      for (const message of messages) {
        if (message.info?.role !== "assistant") continue;
        
        const msgId = message.info?.id;
        if (this.lastMessageIds[sessionId] === msgId) continue;
        
        this.lastMessageIds[sessionId] = msgId;
        state.lastActivity = Date.now();
        state.lastMessageTime = new Date().toISOString();
        
        const text = extractText(message);
        if (!text) continue;

        const learnings = parseLearnings(text);
        
        if (learnings.length > 0) {
          state.learningsCount += learnings.length;
          
          for (const learning of learnings) {
            this.allLearnings.push({
              sessionId,
              ...learning,
              timestamp: new Date().toISOString()
            });
            
            // Persist to findings database
            await appendFinding({
              sessionId,
              category: learning.category,
              description: learning.description
            });
            
            console.log(`\n[${learning.category}] ${sessionId.slice(-8)}`);
            console.log(`  ${learning.description.slice(0, 120)}`);
            
            await logSession(sessionId, `[LEARNING] ${learning.category}: ${learning.description.slice(0, 100)}`);
          }
        }

        if (text.includes("[COMPLETE]")) {
          console.log(`\n[COMPLETE] ${sessionId.slice(-8)}`);
          await logSession(sessionId, "[COMPLETE]");
          state.status = "completed";
          await updateSessionRegistry(sessionId, { status: "completed", completedAt: Date.now() });
          this.sessionIds = this.sessionIds.filter((id) => id !== sessionId);
        }
        
        // Check for explicit abort signal
        if (text.includes("[ABORT]") || text.includes("[TIMEOUT]")) {
          console.log(`\n[ABORT] ${sessionId.slice(-8)}`);
          await logSession(sessionId, "[ABORT]");
          state.status = "aborted";
          await updateSessionRegistry(sessionId, { status: "aborted", abortedAt: Date.now() });
          this.sessionIds = this.sessionIds.filter((id) => id !== sessionId);
        }
      }
    } catch (error) {
      await logSupervisor(`monitor error session=${sessionId} ${error.message}`);
    }
  }

  async checkStuckSessions() {
    const now = Date.now();
    
    for (const sessionId of this.sessionIds) {
      const state = this.sessionStates[sessionId];
      if (!state || state.status !== "running") continue;
      
      const idleTime = now - state.lastActivity;
      const totalTime = now - state.startTime;
      
      // Check if stuck (no output for too long)
      if (idleTime > STUCK_THRESHOLDS.noOutputMinutes * 60000) {
        console.log(`\n[STUCK] ${sessionId.slice(-8)} - No output for ${Math.floor(idleTime/60000)}min`);
        await logSupervisor(`session-stuck session=${sessionId} reason=no_output duration=${Math.floor(idleTime/60000)}min`);
        await this.abortSession(sessionId, "no_output");
        continue;
      }
      
      // Check if exceeded max duration
      if (totalTime > STUCK_THRESHOLDS.maxDurationMinutes * 60000) {
        console.log(`\n[TIMEOUT] ${sessionId.slice(-8)} - Exceeded ${STUCK_THRESHOLDS.maxDurationMinutes}min limit`);
        await logSupervisor(`session-timeout session=${sessionId} duration=${Math.floor(totalTime/60000)}min`);
        await this.abortSession(sessionId, "timeout");
        continue;
      }
      
      // Check if running but no learnings for too long (might be stuck in a loop)
      if (totalTime > 20 * 60000 && state.learningsCount === 0) {
        console.log(`\n[STUCK] ${sessionId.slice(-8)} - No learnings after 20min`);
        await logSupervisor(`session-stuck session=${sessionId} reason=no_learnings`);
        await this.abortSession(sessionId, "no_learnings");
      }
    }
  }

  async abortSession(sessionId, reason) {
    const state = this.sessionStates[sessionId];
    state.status = "aborted";
    
    // Send abort message to session
    try {
      await this.client.session.promptAsync({
        path: { id: sessionId },
        query: { directory: DIRECTORY },
        body: {
          parts: [{ 
            type: "text", 
            text: `[SYSTEM] Session aborted: ${reason}. Please wrap up and mark [COMPLETE] with current findings.` 
          }],
          agent: "explore",
          model: { providerID: "zai-coding-plan", modelID: "glm-4.7" }
        }
      });
    } catch (error) {
      console.error(`Failed to send abort to ${sessionId}:`, error.message);
    }
    
    // Update registry
    await updateSessionRegistry(sessionId, { 
      status: "aborted", 
      abortedAt: Date.now(),
      abortReason: reason,
      learningsCount: state.learningsCount
    });
    
    // Remove from active monitoring
    this.sessionIds = this.sessionIds.filter((id) => id !== sessionId);
    
    console.log(`Aborted ${sessionId.slice(-8)}: ${reason}`);
  }

  async printSummary() {
    if (this.allLearnings.length === 0) {
      console.log("\n\n=== Research Summary ===");
      console.log("No learnings recorded");
      return;
    }
    
    console.log("\n\n=== Research Summary ===");
    console.log(`Session learnings: ${this.allLearnings.length}`);
    
    const byCategory = {};
    this.allLearnings.forEach((l) => {
      byCategory[l.category] = (byCategory[l.category] || 0) + 1;
    });
    
    console.log("\nBy category:");
    Object.entries(byCategory)
      .sort((a, b) => b[1] - a[1])
      .forEach(([cat, count]) => {
        console.log(`  ${cat}: ${count}`);
      });
    
    // Show persisted stats
    const stats = await getStats();
    console.log(`\nTotal persisted findings: ${stats.totalFindings}`);
    console.log(`Active sessions tracked: ${stats.activeSessions}`);
    
    // Show session states
    console.log("\nSession states:");
    Object.entries(this.sessionStates).forEach(([id, state]) => {
      const duration = Math.floor((Date.now() - state.startTime) / 60000);
      console.log(`  ${id.slice(-8)}: ${state.status}, ${state.learningsCount} learnings, ${duration}min`);
    });
  }

  sleep(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }
}

async function main() {
  const { args } = parseArgs(process.argv.slice(2));
  const sessionIds = args.filter((arg) => arg.startsWith("ses_"));
  
  if (sessionIds.length === 0) {
    console.error("Usage: research-monitor <session_id1> [session_id2...]");
    process.exit(1);
  }

  const monitor = new ResearchMonitor(sessionIds);
  await monitor.start();
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
