#!/usr/bin/env node
import { loadFindings, loadIndex, getStats } from "./lib/findings.js";
import { parseArgs } from "./lib/args.js";

async function main() {
  const { args, options } = parseArgs(process.argv.slice(2));
  const command = args[0] || "list";
  
  switch (command) {
    case "list":
    case "ls": {
      const sessionId = options.session || options.id;
      const category = options.category;
      const limit = Number(options.limit || 20);
      
      const findings = await loadFindings({ sessionId, category, limit });
      
      if (findings.length === 0) {
        console.log("No findings found.");
        return;
      }
      
      console.log(`=== Recent Findings (${findings.length}) ===\n`);
      
      for (const finding of findings) {
        const shortId = finding.sessionId.slice(-8);
        const time = new Date(finding.timestamp).toLocaleTimeString();
        console.log(`[${finding.category}] ${shortId} @ ${time}`);
        console.log(`  ${finding.description}`);
        console.log();
      }
      break;
    }
    
    case "stats":
    case "summary": {
      const stats = await getStats();
      const index = await loadIndex();
      
      console.log("=== Findings Statistics ===\n");
      console.log(`Total findings: ${stats.totalFindings}`);
      console.log(`Active sessions: ${stats.activeSessions}`);
      console.log(`Last update: ${stats.lastUpdate || "never"}`);
      
      if (Object.keys(stats.byCategory).length > 0) {
        console.log("\nBy category:");
        Object.entries(stats.byCategory)
          .sort((a, b) => b[1] - a[1])
          .forEach(([cat, count]) => {
            console.log(`  ${cat}: ${count}`);
          });
      }
      
      if (Object.keys(index.bySession).length > 0) {
        console.log("\nBy session:");
        Object.entries(index.bySession)
          .sort((a, b) => new Date(b[1].lastAt) - new Date(a[1].lastAt))
          .slice(0, 10)
          .forEach(([sessionId, data]) => {
            const shortId = sessionId.slice(-8);
            const lastAt = new Date(data.lastAt).toLocaleString();
            console.log(`  ${shortId}: ${data.count} findings (last: ${lastAt})`);
          });
      }
      break;
    }
    
    case "export": {
      const format = options.format || "json";
      const findings = await loadFindings({ limit: 10000 });
      
      if (format === "json") {
        console.log(JSON.stringify(findings, null, 2));
      } else if (format === "csv") {
        console.log("id,timestamp,sessionId,category,description");
        for (const f of findings) {
          const desc = f.description.replace(/"/g, '""').replace(/\n/g, " ");
          console.log(`"${f.id}","${f.timestamp}","${f.sessionId}","${f.category}","${desc}"`);
        }
      } else {
        console.error(`Unknown format: ${format}. Use json or csv.`);
        process.exit(1);
      }
      break;
    }
    
    default:
      console.log("Usage: findings <command> [options]");
      console.log("\nCommands:");
      console.log("  list, ls          List recent findings");
      console.log("  stats, summary    Show statistics");
      console.log("  export            Export findings (use --format json|csv)");
      console.log("\nOptions:");
      console.log("  --session <id>   Filter by session ID");
      console.log("  --category <cat> Filter by category (BUG, SECURITY, etc.)");
      console.log("  --limit <n>      Limit results (default: 20)");
      console.log("  --format <fmt>   Export format: json or csv");
  }
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
