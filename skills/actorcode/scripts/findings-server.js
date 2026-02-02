#!/usr/bin/env node
/**
 * Findings Export Server
 * Serves findings data as JSON for the web dashboard
 */

import http from "http";
import fs from "fs/promises";
import path from "path";
import { loadFindings, getStats, loadIndex } from "./lib/findings.js";
import { createClient } from "./lib/client.js";
import { summarizeMessages, summarizeMessagesStream } from "./lib/summary.js";
import { loadEnvFile } from "./lib/env.js";

const PORT = 8765;
const FINDINGS_LOG = path.join(process.cwd(), ".actorcode", "findings", "findings.log.jsonl");
const DIRECTORY = process.cwd();

await loadEnvFile();

async function readFindingsLog() {
  try {
    const raw = await fs.readFile(FINDINGS_LOG, "utf8");
    return raw
      .trim()
      .split("\n")
      .filter(Boolean)
      .map(line => JSON.parse(line));
  } catch (error) {
    if (error.code === "ENOENT") return [];
    throw error;
  }
}

async function getSessions() {
  const { loadRegistry } = await import("./lib/registry.js");
  const registry = await loadRegistry();
  return Object.entries(registry.sessions)
    .filter(([id]) => id.startsWith("ses_"))
    .map(([id, data]) => ({
      sessionId: id,
      title: data.title || "Unknown",
      status: data.status || "unknown",
      agent: data.agent,
      tier: data.tier,
      createdAt: data.createdAt,
      lastEventAt: data.lastEventAt
    }));
}

async function getSessionMessages(sessionId) {
  const client = createClient();
  try {
    const messagesResponse = await client.session.messages({
      path: { id: sessionId },
      query: { directory: DIRECTORY, limit: 500 }
    });
    return messagesResponse.data || [];
  } catch (error) {
    console.error(`Failed to fetch messages for ${sessionId}:`, error.message);
    return [];
  }
}

async function generateSummary(sessionId) {
  try {
    const messages = await getSessionMessages(sessionId);
    const summary = await summarizeMessages(messages, { model: "glm-4.7-flash" });
    return { markdown: summary.markdown, model: summary.model, truncated: summary.truncated };
  } catch (error) {
    console.error(`Failed to generate summary for ${sessionId}:`, error.message);
    return { error: error.message };
  }
}

const server = http.createServer(async (req, res) => {
  // CORS headers
  res.setHeader("Access-Control-Allow-Origin", "*");
  res.setHeader("Access-Control-Allow-Methods", "GET, OPTIONS");
  res.setHeader("Access-Control-Allow-Headers", "Content-Type");
  
  if (req.method === "OPTIONS") {
    res.writeHead(200);
    res.end();
    return;
  }
  
  try {
    if (req.url === "/api/findings") {
      const findings = await readFindingsLog();
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify(findings.reverse()));
      
    } else if (req.url === "/api/stats") {
      const stats = await getStats();
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify(stats));
      
    } else if (req.url === "/api/sessions") {
      const sessions = await getSessions();
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify(sessions));
      
    } else if (req.url === "/api/all") {
      const [findings, stats, sessions] = await Promise.all([
        readFindingsLog(),
        getStats(),
        getSessions()
      ]);
      
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({
        findings: findings.reverse(),
        stats,
        sessions
      }));
      
    } else if (req.url.startsWith("/api/messages?sessionId=")) {
      const url = new URL(req.url, `http://localhost:${PORT}`);
      const sessionId = url.searchParams.get("sessionId");
      
      if (!sessionId) {
        res.writeHead(400, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ error: "sessionId required" }));
        return;
      }
      
      const messages = await getSessionMessages(sessionId);
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ sessionId, messages }));
      
    } else if (req.url.startsWith("/api/summary?sessionId=")) {
      const url = new URL(req.url, `http://localhost:${PORT}`);
      const sessionId = url.searchParams.get("sessionId");
      const stream = url.searchParams.get("stream") === "true";
      
      if (!sessionId) {
        res.writeHead(400, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ error: "sessionId required" }));
        return;
      }
      
      if (stream) {
        // SSE streaming endpoint
        res.writeHead(200, {
          "Content-Type": "text/event-stream",
          "Cache-Control": "no-cache",
          "Connection": "keep-alive",
          "Access-Control-Allow-Origin": "*"
        });
        
        try {
          const messages = await getSessionMessages(sessionId);
          const streamGenerator = summarizeMessagesStream(messages, { model: "glm-4.7-flash" });
          
          for await (const chunk of streamGenerator) {
            res.write(`data: ${JSON.stringify({ chunk })}\n\n`);
          }
          
          res.write(`data: ${JSON.stringify({ done: true })}\n\n`);
        } catch (error) {
          res.write(`data: ${JSON.stringify({ error: error.message })}\n\n`);
        }
        
        res.end();
      } else {
        // Non-streaming (legacy)
        const summary = await generateSummary(sessionId);
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ sessionId, ...summary }));
      }
      
    } else {
      res.writeHead(404, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ error: "Not found" }));
    }
  } catch (error) {
    console.error("Error:", error.message);
    res.writeHead(500, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ error: error.message }));
  }
});

server.listen(PORT, () => {
  console.log(`Findings API server running on http://localhost:${PORT}`);
  console.log("Endpoints:");
  console.log(`  http://localhost:${PORT}/api/findings`);
  console.log(`  http://localhost:${PORT}/api/stats`);
  console.log(`  http://localhost:${PORT}/api/sessions`);
  console.log(`  http://localhost:${PORT}/api/all`);
  console.log(`  http://localhost:${PORT}/api/messages?sessionId=<id>`);
  console.log(`  http://localhost:${PORT}/api/summary?sessionId=<id>`);
  console.log(`  http://localhost:${PORT}/api/summary?sessionId=<id>&stream=true`);
});
