#!/usr/bin/env node
/**
 * Session Cleanup Utility
 * Removes old/orphaned sessions from registry
 */

import { loadRegistry, saveRegistry } from "./lib/registry.js";
import { parseArgs } from "./lib/args.js";

async function main() {
  const { args, options } = parseArgs(process.argv.slice(2));
  const dryRun = options["dry-run"] || options.dryRun;
  const olderThan = options.olderThan || options["older-than"] || "1h";
  const status = options.status;
  
  // Parse time threshold
  const match = olderThan.match(/^(\d+)([hdm])$/);
  if (!match) {
    console.error("Invalid --older-than format. Use: 1h, 30m, 7d");
    process.exit(1);
  }
  
  const [, amount, unit] = match;
  const multipliers = { m: 60 * 1000, h: 60 * 60 * 1000, d: 24 * 60 * 60 * 1000 };
  const threshold = Date.now() - (parseInt(amount) * multipliers[unit]);
  
  const registry = await loadRegistry();
  const sessions = Object.entries(registry.sessions);
  
  const toRemove = sessions.filter(([id, data]) => {
    // Never remove active/busy sessions unless forced
    if (!options.force && (data.status === "running" || data.status === "busy")) {
      return false;
    }
    
    // Filter by status if specified
    if (status && data.status !== status) {
      return false;
    }
    
    // Check age
    const lastActivity = data.lastEventAt || data.createdAt || 0;
    return lastActivity < threshold;
  });
  
  if (toRemove.length === 0) {
    console.log("No sessions to clean up.");
    return;
  }
  
  console.log(`Found ${toRemove.length} sessions to remove (older than ${olderThan}):`);
  
  for (const [id, data] of toRemove) {
    const age = data.lastEventAt || data.createdAt 
      ? new Date(data.lastEventAt || data.createdAt).toISOString()
      : "unknown";
    console.log(`  ${id.slice(-8)} - ${data.status || "unknown"} - ${age}`);
  }
  
  if (dryRun) {
    console.log("\n(Dry run - no changes made)");
    return;
  }
  
  // Remove sessions
  for (const [id] of toRemove) {
    delete registry.sessions[id];
  }
  
  registry.updatedAt = Date.now();
  await saveRegistry(registry);
  
  console.log(`\n✓ Removed ${toRemove.length} sessions`);
  console.log(`  Registry now has ${Object.keys(registry.sessions).length} sessions`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
