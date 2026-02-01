import fs from "fs/promises";
import path from "path";

const FINDINGS_DIR = path.join(process.cwd(), ".actorcode", "findings");
const FINDINGS_LOG = path.join(FINDINGS_DIR, "findings.log.jsonl");
const FINDINGS_INDEX = path.join(FINDINGS_DIR, "index.json");

async function ensureDir() {
  await fs.mkdir(FINDINGS_DIR, { recursive: true });
}

// Rate limiting: max findings per session per hour
const RATE_LIMITS = {
  perSession: 50,      // Max findings per session
  perHour: 100,        // Max findings per session per hour
  minInterval: 5000    // Minimum ms between findings from same session
};

const sessionFindings = new Map(); // sessionId -> { count, lastTime, hourlyCount, hourStart }

async function checkRateLimit(sessionId) {
  const now = Date.now();
  const hourAgo = now - 3600000;
  
  if (!sessionFindings.has(sessionId)) {
    sessionFindings.set(sessionId, { count: 0, lastTime: 0, hourlyCount: 0, hourStart: now });
  }
  
  const session = sessionFindings.get(sessionId);
  
  // Reset hourly counter if hour has passed
  if (now - session.hourStart > 3600000) {
    session.hourlyCount = 0;
    session.hourStart = now;
  }
  
  // Check limits
  if (session.count >= RATE_LIMITS.perSession) {
    return { allowed: false, reason: `Session ${sessionId} exceeded ${RATE_LIMITS.perSession} findings` };
  }
  
  if (session.hourlyCount >= RATE_LIMITS.perHour) {
    return { allowed: false, reason: `Session ${sessionId} exceeded ${RATE_LIMITS.perHour} findings/hour` };
  }
  
  if (now - session.lastTime < RATE_LIMITS.minInterval) {
    return { allowed: false, reason: `Session ${sessionId} finding too frequent` };
  }
  
  // Update counters
  session.count++;
  session.hourlyCount++;
  session.lastTime = now;
  
  return { allowed: true };
}

export async function appendFinding(finding) {
  await ensureDir();
  
  // Check rate limit
  const rateCheck = await checkRateLimit(finding.sessionId);
  if (!rateCheck.allowed) {
    console.warn(`[findings] Rate limit: ${rateCheck.reason}`);
    return null;
  }
  
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
