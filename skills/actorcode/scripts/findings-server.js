#!/usr/bin/env node
/**
 * Findings Export Server
 * Serves findings data as JSON for the web dashboard
 */

import http from "http";
import fs from "fs/promises";
import path from "path";
import { loadFindings, getStats, loadIndex } from "./lib/findings.js";

const PORT = 8765;
const FINDINGS_LOG = path.join(process.cwd(), ".actorcode", "findings", "findings.log.jsonl");

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
});
