#!/usr/bin/env node
/**
 * Clean up garbage findings from the database
 * Removes duplicates, file descriptions, and low-value entries
 */

import fs from "fs/promises";
import path from "path";

const FINDINGS_FILE = path.join(process.cwd(), ".actorcode", "findings", "findings.log.jsonl");
const INDEX_FILE = path.join(process.cwd(), ".actorcode", "findings", "index.json");

// Patterns that indicate garbage findings
const GARBAGE_PATTERNS = [
  // File descriptions (just describing what a file contains)
  /^[^:]+:\s*[^:]+\s+is\s+a\s+/i,
  /^[^:]+:\s*The\s+[^:]+\s+(file|module|component|function)/i,
  /^[^:]+:\s*This\s+[^:]+\s+(file|module|component|function)/i,
  /^[^:]+:\s*[^:]+\s+provides\s+/i,
  /^[^:]+:\s*[^:]+\s+implements\s+/i,
  /^[^:]+:\s*[^:]+\s+demonstrates\s+/i,
  /^[^:]+:\s*[^:]+\s+uses\s+/i,
  /^[^:]+:\s*[^:]+\s+shows\s+/i,
  
  // Generic/empty descriptions
  /^[^:]+:\s*\[?\s*\]?\s*$/,
  /^[^:]+:\s*See\s+/i,
  /^[^:]+:\s*Check\s+/i,
  /^[^:]+:\s*Look\s+/i,
  
  // Obvious facts
  /^[^:]+:\s*Code\s+is\s+written\s+in\s+/i,
  /^[^:]+:\s*Project\s+uses\s+/i,
  /^[^:]+:\s*The\s+repository\s+contains/i,
  
  // Truncated entries (ending mid-sentence)
  /:\s*[^,]+,$/,
  
  // Repeated phrases (duplicates within same session)
];

// Sessions known to have garbage data
const GARBAGE_SESSIONS = new Set([
  "ses_3e5f9b0eaffeXtABl3TMdqdlcx"  // The pico session with 3,867 file descriptions
]);

async function loadFindings() {
  try {
    const raw = await fs.readFile(FINDINGS_FILE, "utf8");
    return raw.trim().split("\n").filter(Boolean).map(line => {
      try {
        return JSON.parse(line);
      } catch (e) {
        return null;
      }
    }).filter(Boolean);
  } catch (error) {
    if (error.code === "ENOENT") return [];
    throw error;
  }
}

function isGarbage(finding) {
  const desc = finding.description || "";
  
  // Check if from known garbage session
  if (GARBAGE_SESSIONS.has(finding.sessionId)) {
    // Keep SECURITY and BUG findings even from garbage sessions
    if (["SECURITY", "BUG", "PERFORMANCE"].includes(finding.category)) {
      return false;
    }
    return true;
  }
  
  // Check against garbage patterns
  for (const pattern of GARBAGE_PATTERNS) {
    if (pattern.test(desc)) {
      return true;
    }
  }
  
  // Check for very short descriptions (less than 20 chars after category)
  const content = desc.replace(/^[^:]+:\s*/, "");
  if (content.length < 20) {
    return true;
  }
  
  return false;
}

function deduplicate(findings) {
  const seen = new Set();
  return findings.filter(f => {
    // Create a fingerprint based on normalized description
    const normalized = (f.description || "")
      .toLowerCase()
      .replace(/\s+/g, " ")
      .replace(/[^a-z0-9]/g, "")
      .slice(0, 100);
    
    if (seen.has(normalized)) {
      return false;
    }
    seen.add(normalized);
    return true;
  });
}

async function rebuildIndex(findings) {
  const index = {
    version: 1,
    updatedAt: Date.now(),
    bySession: {},
    byCategory: {},
    total: findings.length
  };
  
  for (const finding of findings) {
    // By session
    if (!index.bySession[finding.sessionId]) {
      index.bySession[finding.sessionId] = { count: 0, lastAt: null };
    }
    index.bySession[finding.sessionId].count++;
    if (!index.bySession[finding.sessionId].lastAt || 
        new Date(finding.timestamp) > new Date(index.bySession[finding.sessionId].lastAt)) {
      index.bySession[finding.sessionId].lastAt = finding.timestamp;
    }
    
    // By category
    const category = finding.category || "UNKNOWN";
    index.byCategory[category] = (index.byCategory[category] || 0) + 1;
  }
  
  await fs.writeFile(INDEX_FILE, JSON.stringify(index, null, 2) + "\n", "utf8");
  return index;
}

async function main() {
  console.log("Loading findings database...");
  const findings = await loadFindings();
  console.log(`Total findings: ${findings.length}`);
  
  console.log("\nFiltering garbage entries...");
  const cleanFindings = findings.filter(f => !isGarbage(f));
  const removedCount = findings.length - cleanFindings.length;
  console.log(`Removed ${removedCount} garbage entries`);
  
  console.log("\nDeduplicating...");
  const dedupedFindings = deduplicate(cleanFindings);
  const dupesRemoved = cleanFindings.length - dedupedFindings.length;
  console.log(`Removed ${dupesRemoved} duplicates`);
  
  console.log("\nFinal count:", dedupedFindings.length);
  
  // Backup original
  const backupPath = FINDINGS_FILE + ".backup." + Date.now();
  await fs.copyFile(FINDINGS_FILE, backupPath);
  console.log(`\nBackup saved to: ${backupPath}`);
  
  // Write cleaned data
  const lines = dedupedFindings.map(f => JSON.stringify(f)).join("\n") + "\n";
  await fs.writeFile(FINDINGS_FILE, lines, "utf8");
  
  // Rebuild index
  const index = await rebuildIndex(dedupedFindings);
  console.log("\nIndex rebuilt");
  
  // Report
  console.log("\n=== Cleanup Report ===");
  console.log(`Original: ${findings.length} findings`);
  console.log(`Garbage removed: ${removedCount}`);
  console.log(`Duplicates removed: ${dupesRemoved}`);
  console.log(`Final: ${dedupedFindings.length} findings`);
  console.log(`\nBy category:`);
  Object.entries(index.byCategory)
    .sort((a, b) => b[1] - a[1])
    .forEach(([cat, count]) => {
      console.log(`  ${cat}: ${count}`);
    });
  console.log(`\nBy session:`);
  Object.entries(index.bySession)
    .sort((a, b) => b[1].count - a[1].count)
    .forEach(([sess, data]) => {
      console.log(`  ${sess.slice(-12)}: ${data.count}`);
    });
}

main().catch(error => {
  console.error("Error:", error.message);
  process.exit(1);
});
