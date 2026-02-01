import fs from "fs/promises";
import path from "path";

const FINDINGS_DIR = path.join(process.cwd(), ".actorcode", "findings");
const FINDINGS_LOG = path.join(FINDINGS_DIR, "findings.log.jsonl");
const FINDINGS_INDEX = path.join(FINDINGS_DIR, "index.json");

async function ensureDir() {
  await fs.mkdir(FINDINGS_DIR, { recursive: true });
}

export async function appendFinding(finding) {
  await ensureDir();
  
  const entry = {
    id: `fnd_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
    timestamp: new Date().toISOString(),
    ...finding
  };
  
  // Append to log file
  await fs.appendFile(FINDINGS_LOG, JSON.stringify(entry) + "\n", "utf8");
  
  // Update index
  await updateIndex(entry);
  
  return entry;
}

async function updateIndex(entry) {
  let index = { version: 1, updatedAt: Date.now(), bySession: {}, byCategory: {}, total: 0 };
  
  try {
    const raw = await fs.readFile(FINDINGS_INDEX, "utf8");
    index = JSON.parse(raw);
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  
  // Update by session
  if (!index.bySession[entry.sessionId]) {
    index.bySession[entry.sessionId] = { count: 0, lastAt: null };
  }
  index.bySession[entry.sessionId].count++;
  index.bySession[entry.sessionId].lastAt = entry.timestamp;
  
  // Update by category
  const category = entry.category || "UNKNOWN";
  if (!index.byCategory[category]) {
    index.byCategory[category] = 0;
  }
  index.byCategory[category]++;
  
  index.total++;
  index.updatedAt = Date.now();
  
  await fs.writeFile(FINDINGS_INDEX, JSON.stringify(index, null, 2) + "\n", "utf8");
}

export async function loadFindings(options = {}) {
  await ensureDir();
  
  const { sessionId, category, limit = 100, since } = options;
  const findings = [];
  
  try {
    const raw = await fs.readFile(FINDINGS_LOG, "utf8");
    const lines = raw.trim().split("\n").filter(Boolean);
    
    for (const line of lines.reverse()) {
      if (findings.length >= limit) break;
      
      try {
        const entry = JSON.parse(line);
        
        if (sessionId && entry.sessionId !== sessionId) continue;
        if (category && entry.category !== category) continue;
        if (since && new Date(entry.timestamp) < new Date(since)) continue;
        
        findings.push(entry);
      } catch (e) {
        // Skip malformed lines
      }
    }
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  
  return findings.reverse();
}

export async function loadIndex() {
  await ensureDir();
  
  try {
    const raw = await fs.readFile(FINDINGS_INDEX, "utf8");
    return JSON.parse(raw);
  } catch (error) {
    if (error.code === "ENOENT") {
      return { version: 1, updatedAt: 0, bySession: {}, byCategory: {}, total: 0 };
    }
    throw error;
  }
}

export async function getStats() {
  const index = await loadIndex();
  return {
    totalFindings: index.total,
    byCategory: index.byCategory,
    activeSessions: Object.keys(index.bySession).length,
    lastUpdate: index.updatedAt ? new Date(index.updatedAt).toISOString() : null
  };
}

export function findingsPath() {
  return FINDINGS_DIR;
}

export function findingsLogPath() {
  return FINDINGS_LOG;
}
