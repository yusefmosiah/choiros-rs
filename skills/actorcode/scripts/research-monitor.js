#!/usr/bin/env node
import { createClient } from "./lib/client.js";
import { logSupervisor, logSession } from "./lib/logs.js";
import { parseArgs } from "./lib/args.js";
import { appendFinding, getStats } from "./lib/findings.js";

const DIRECTORY = process.cwd();

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
  }

  async start() {
    await logSupervisor(`research-monitor start sessions=${this.sessionIds.length}`);
    
    process.on("SIGINT", () => {
      this.running = false;
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
      
      for (const message of messages) {
        if (message.info?.role !== "assistant") continue;
        
        const msgId = message.info?.id;
        if (this.lastMessageIds[sessionId] === msgId) continue;
        
        this.lastMessageIds[sessionId] = msgId;
        
        const text = extractText(message);
        if (!text) continue;

        const learnings = parseLearnings(text);
        
        if (learnings.length > 0) {
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
          this.sessionIds = this.sessionIds.filter((id) => id !== sessionId);
        }
      }
    } catch (error) {
      await logSupervisor(`monitor error session=${sessionId} ${error.message}`);
    }
  }

  async printSummary() {
    if (this.allLearnings.length === 0) return;
    
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
