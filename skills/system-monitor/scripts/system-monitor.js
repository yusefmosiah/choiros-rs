#!/usr/bin/env node
/**
 * System Monitor - Actor Network Visualizer
 * 
 * Generates ASCII diagrams of the actor network for chat context
 * or saves to file as evidence/report.
 */

const fs = require('fs');
const path = require('path');

// Configuration
const REGISTRY_PATH = path.join(process.cwd(), '.actorcode', 'registry.json');
const LOGS_DIR = path.join(process.cwd(), 'logs', 'actorcode');
const OUTPUT_FILE = path.join(process.cwd(), 'reports', 'actor-network.md');

// ANSI colors
const COLORS = {
  reset: '\x1b[0m',
  bright: '\x1b[1m',
  dim: '\x1b[2m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  magenta: '\x1b[35m',
  cyan: '\x1b[36m',
};

// Actor tier symbols
const TIER_SYMBOLS = {
  pico: '○',
  nano: '◐',
  micro: '◑',
  milli: '◒',
  unknown: '◎'
};

// Status indicators
const STATUS_SYMBOLS = {
  running: '▶',
  completed: '✓',
  failed: '✗',
  aborted: '⊘',
  unknown: '?'
};

/**
 * Load actor registry
 */
function loadRegistry() {
  try {
    if (!fs.existsSync(REGISTRY_PATH)) {
      return { sessions: {} };
    }
    const data = fs.readFileSync(REGISTRY_PATH, 'utf8');
    return JSON.parse(data);
  } catch (err) {
    console.error(`${COLORS.red}Error loading registry:${COLORS.reset}`, err.message);
    return { sessions: {} };
  }
}

/**
 * Get session log file info
 */
function getSessionLogInfo(sessionId) {
  const logPath = path.join(LOGS_DIR, `${sessionId}.log`);
  try {
    if (!fs.existsSync(logPath)) {
      return null;
    }
    const stats = fs.statSync(logPath);
    const content = fs.readFileSync(logPath, 'utf8');
    const lines = content.split('\n').filter(l => l.trim());
    
    // Extract last activity
    const lastLines = lines.slice(-5);
    const lastActivity = lastLines.find(l => l.includes('[') && l.includes(']')) || 'No recent activity';
    
    return {
      size: stats.size,
      lines: lines.length,
      lastActivity: lastActivity.slice(0, 60),
      modified: stats.mtime
    };
  } catch (err) {
    return null;
  }
}

/**
 * Format duration from milliseconds
 */
function formatDuration(ms) {
  if (!ms || ms < 0) return 'unknown';
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);
  
  if (days > 0) return `${days}d ${hours % 24}h`;
  if (hours > 0) return `${hours}h ${minutes % 60}m`;
  if (minutes > 0) return `${minutes}m ${seconds % 60}s`;
  return `${seconds}s`;
}

/**
 * Get status color
 */
function getStatusColor(status) {
  switch (status) {
    case 'running': return COLORS.green;
    case 'completed': return COLORS.blue;
    case 'failed': return COLORS.red;
    case 'aborted': return COLORS.yellow;
    default: return COLORS.dim;
  }
}

/**
 * Generate ASCII network diagram
 */
function generateNetworkDiagram(registry, options = {}) {
  const { sessions = {} } = registry;
  const sessionIds = Object.keys(sessions);
  
  if (sessionIds.length === 0) {
    return 'No active actors in registry.';
  }
  
  const lines = [];
  const now = Date.now();
  
  // Header
  lines.push('╔════════════════════════════════════════════════════════════════╗');
  lines.push('║              ACTOR NETWORK - System Monitor                    ║');
  lines.push('╠════════════════════════════════════════════════════════════════╣');
  lines.push(`║  Generated: ${new Date().toISOString()}                              ║`);
  lines.push(`║  Total Actors: ${sessionIds.length.toString().padEnd(3)}                                      ║`);
  lines.push('╚════════════════════════════════════════════════════════════════╝');
  lines.push('');
  
  // Group by status
  const byStatus = {};
  sessionIds.forEach(id => {
    const status = sessions[id].status || 'unknown';
    if (!byStatus[status]) byStatus[status] = [];
    byStatus[status].push(id);
  });
  
  // Status summary
  lines.push('┌─ Status Summary ──────────────────────────────────────────────┐');
  Object.entries(byStatus).forEach(([status, ids]) => {
    const color = getStatusColor(status);
    const symbol = STATUS_SYMBOLS[status] || STATUS_SYMBOLS.unknown;
    lines.push(`│ ${color}${symbol}${COLORS.reset} ${status.padEnd(12)}: ${ids.length.toString().padStart(3)} actors`);
  });
  lines.push('└────────────────────────────────────────────────────────────────┘');
  lines.push('');
  
  // Actor details
  lines.push('┌─ Actor Details ───────────────────────────────────────────────┐');
  
  sessionIds.forEach((id, idx) => {
    const session = sessions[id];
    const tier = session.tier || 'unknown';
    const status = session.status || 'unknown';
    const title = session.title || 'Untitled';
    
    const tierSymbol = TIER_SYMBOLS[tier] || TIER_SYMBOLS.unknown;
    const statusSymbol = STATUS_SYMBOLS[status] || STATUS_SYMBOLS.unknown;
    const statusColor = getStatusColor(status);
    
    // Calculate runtime
    const created = session.created_at ? new Date(session.created_at).getTime() : now;
    const runtime = now - created;
    
    // Get log info
    const logInfo = getSessionLogInfo(id);
    
    // Actor box
    lines.push(`│`);
    lines.push(`│  ${tierSymbol} ${id.slice(0, 8)}...  ${statusColor}${statusSymbol} ${status}${COLORS.reset}`);
    lines.push(`│     Title: ${title.slice(0, 40)}`);
    lines.push(`│     Tier:  ${tier.padEnd(10)}  Runtime: ${formatDuration(runtime)}`);
    
    if (logInfo) {
      lines.push(`│     Log:   ${logInfo.lines} lines, ${(logInfo.size / 1024).toFixed(1)} KB`);
      lines.push(`│     Last:  ${logInfo.lastActivity.slice(0, 50)}`);
    }
    
    if (idx < sessionIds.length - 1) {
      lines.push(`│     ${COLORS.dim}────────────────────────────────────────${COLORS.reset}`);
    }
  });
  
  lines.push('│');
  lines.push('└────────────────────────────────────────────────────────────────┘');
  lines.push('');
  
  // Hierarchy view (if parent-child relationships exist)
  const withParent = sessionIds.filter(id => sessions[id].parent_id);
  if (withParent.length > 0) {
    lines.push('┌─ Actor Hierarchy ─────────────────────────────────────────────┐');
    
    // Build tree
    const roots = sessionIds.filter(id => !sessions[id].parent_id);
    
    function printTree(actorId, depth = 0) {
      const session = sessions[actorId];
      const indent = '│   '.repeat(depth);
      const tier = session.tier || 'unknown';
      const status = session.status || 'unknown';
      const tierSymbol = TIER_SYMBOLS[tier] || TIER_SYMBOLS.unknown;
      const statusSymbol = STATUS_SYMBOLS[status] || STATUS_SYMBOLS.unknown;
      const statusColor = getStatusColor(status);
      
      lines.push(`│ ${indent}${tierSymbol} ${actorId.slice(0, 8)} ${statusColor}${statusSymbol}${COLORS.reset} ${session.title?.slice(0, 30) || ''}`);
      
      // Find children
      const children = sessionIds.filter(id => sessions[id].parent_id === actorId);
      children.forEach(childId => printTree(childId, depth + 1));
    }
    
    roots.forEach(rootId => printTree(rootId));
    lines.push('└────────────────────────────────────────────────────────────────┘');
    lines.push('');
  }
  
  // Legend
  lines.push('┌─ Legend ──────────────────────────────────────────────────────┐');
  lines.push('│  Tiers:  ○ pico  ◐ nano  ◑ micro  ◒ milli                    │');
  lines.push('│  Status: ▶ running  ✓ completed  ✗ failed  ⊘ aborted         │');
  lines.push('└────────────────────────────────────────────────────────────────┘');
  
  return lines.join('\n');
}

/**
 * Generate compact view for chat context
 */
function generateCompactView(registry) {
  const { sessions = {} } = registry;
  const sessionIds = Object.keys(sessions);
  
  if (sessionIds.length === 0) {
    return 'No active actors.';
  }
  
  const lines = [];
  const now = Date.now();
  
  lines.push('**Actor Network**');
  lines.push('');
  
  // Status counts
  const counts = {};
  sessionIds.forEach(id => {
    const status = sessions[id].status || 'unknown';
    counts[status] = (counts[status] || 0) + 1;
  });
  
  const statusLine = Object.entries(counts)
    .map(([status, count]) => `${status}: ${count}`)
    .join(' | ');
  lines.push(`Status: ${statusLine}`);
  lines.push('');
  
  // Actor list
  sessionIds.forEach(id => {
    const session = sessions[id];
    const tier = session.tier || '?';
    const status = session.status || '?';
    const title = session.title || 'Untitled';
    const created = session.created_at ? new Date(session.created_at).getTime() : now;
    const runtime = formatDuration(now - created);
    
    lines.push(`- **${id.slice(0, 8)}** (${tier}): ${status} - "${title.slice(0, 30)}" (${runtime})`);
  });
  
  return lines.join('\n');
}

/**
 * Save report to file
 */
function saveReport(content) {
  const reportsDir = path.dirname(OUTPUT_FILE);
  if (!fs.existsSync(reportsDir)) {
    fs.mkdirSync(reportsDir, { recursive: true });
  }
  
  const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
  const filename = path.join(reportsDir, `actor-network-${timestamp}.md`);
  
  const report = `# Actor Network Report

Generated: ${new Date().toISOString()}

\`\`\`
${content}
\`\`\`

---
*Report generated by system-monitor skill*
`;
  
  fs.writeFileSync(filename, report);
  console.log(`${COLORS.green}Report saved to:${COLORS.reset} ${filename}`);
  return filename;
}

/**
 * Main function
 */
function main() {
  const args = process.argv.slice(2);
  const options = {
    compact: args.includes('--compact') || args.includes('-c'),
    save: args.includes('--save') || args.includes('-s'),
    help: args.includes('--help') || args.includes('-h')
  };
  
  if (options.help) {
    console.log(`
System Monitor - Actor Network Visualizer

Usage: node system-monitor.js [options]

Options:
  -c, --compact    Compact view for chat context
  -s, --save       Save report to file
  -h, --help       Show this help

Examples:
  node system-monitor.js              # Full ASCII diagram
  node system-monitor.js --compact    # Compact view
  node system-monitor.js --save       # Save to reports/
`);
    process.exit(0);
  }
  
  // Load registry
  const registry = loadRegistry();
  
  // Generate output
  let output;
  if (options.compact) {
    output = generateCompactView(registry);
    console.log(output);
  } else {
    output = generateNetworkDiagram(registry);
    console.log(output);
  }
  
  // Save if requested
  if (options.save) {
    const filename = saveReport(output);
    console.log(`\n${COLORS.cyan}Evidence saved for:${COLORS.reset} ${filename}`);
  }
}

// Run if called directly
if (require.main === module) {
  main();
}

module.exports = {
  loadRegistry,
  generateNetworkDiagram,
  generateCompactView,
  getSessionLogInfo,
  formatDuration
};
