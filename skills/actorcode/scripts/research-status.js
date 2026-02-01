#!/usr/bin/env node
import { createClient } from "./lib/client.js";
import { loadRegistry } from "./lib/registry.js";
import { parseArgs } from "./lib/args.js";

const DIRECTORY = process.cwd();

const LEARNING_REGEX = /\[LEARNING\]\s*(\w+):\s*(.+?)(?=\[LEARNING\]|\[COMPLETE\]|$)/gs;
const COMPLETE_REGEX = /\[COMPLETE\]/;

function extractLearnings(text) {
  const learnings = [];
  let match;
  while ((match = LEARNING_REGEX.exec(text)) !== null) {
    learnings.push({
      category: match[1].toUpperCase(),
      description: match[2].trim()
    });
  }
  return learnings;
}

function extractText(message) {
  const parts = message?.parts || [];
  return parts
    .filter((part) => part?.type === "text" && part?.text)
    .map((part) => part.text)
    .join("\n");
}

async function getSessionStatus(client, sessionId, registryEntry) {
  try {
    const [messagesResponse, statusResponse] = await Promise.all([
      client.session.messages({
        path: { id: sessionId },
        query: { directory: DIRECTORY, limit: 20 }
      }),
      client.session.status({ query: { directory: DIRECTORY } }).catch(() => ({ data: {} }))
    ]);

    const messages = messagesResponse.data || [];
    const status = statusResponse.data?.[sessionId]?.type || registryEntry?.status || "unknown";
    
    const assistantMessages = messages.filter(m => m.info?.role === "assistant");
    const allText = assistantMessages.map(extractText).join("\n");
    
    const learnings = extractLearnings(allText);
    const isComplete = COMPLETE_REGEX.test(allText);
    
    const lastActivity = messages[0]?.info?.time?.created 
      ? new Date(messages[0].info.time.created)
      : registryEntry?.lastEventAt 
        ? new Date(registryEntry.lastEventAt)
        : null;
    
    return {
      sessionId,
      title: registryEntry?.title || "Unknown",
      status: isComplete ? "completed" : status,
      learningsCount: learnings.length,
      learnings: learnings.slice(0, 5),
      lastActivity,
      agent: registryEntry?.agent,
      tier: registryEntry?.tier,
      createdAt: registryEntry?.createdAt ? new Date(registryEntry.createdAt) : null
    };
  } catch (error) {
    return {
      sessionId,
      title: registryEntry?.title || "Unknown",
      status: "error",
      error: error.message,
      learningsCount: 0,
      lastActivity: null,
      agent: registryEntry?.agent,
      tier: registryEntry?.tier,
      createdAt: registryEntry?.createdAt ? new Date(registryEntry.createdAt) : null
    };
  }
}

function formatDuration(ms) {
  if (!ms || ms < 0) return "unknown";
  const minutes = Math.floor(ms / 60000);
  const hours = Math.floor(minutes / 60);
  if (hours > 0) return `${hours}h ${minutes % 60}m`;
  if (minutes > 0) return `${minutes}m`;
  return "<1m";
}

function formatTimeAgo(date) {
  if (!date) return "unknown";
  const now = new Date();
  const diff = now - date;
  const minutes = Math.floor(diff / 60000);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);
  
  if (days > 0) return `${days}d ago`;
  if (hours > 0) return `${hours}h ago`;
  if (minutes > 0) return `${minutes}m ago`;
  return "just now";
}

async function main() {
  const { options } = parseArgs(process.argv.slice(2));
  const showAll = options.all || options.a;
  const showLearnings = options.learnings || options.l;
  
  const client = createClient();
  const registry = await loadRegistry();
  
  const sessionIds = Object.keys(registry.sessions).filter(id => id.startsWith("ses_"));
  
  if (sessionIds.length === 0) {
    console.log("No research sessions found in registry.");
    return;
  }
  
  console.log("Fetching status for research sessions...\n");
  
  const statuses = await Promise.all(
    sessionIds.map(id => getSessionStatus(client, id, registry.sessions[id]))
  );
  
  // Sort by status (running first), then by last activity
  statuses.sort((a, b) => {
    const statusOrder = { running: 0, spawned: 1, unknown: 2, completed: 3, error: 4 };
    const orderDiff = (statusOrder[a.status] || 5) - (statusOrder[b.status] || 5);
    if (orderDiff !== 0) return orderDiff;
    return (b.lastActivity || 0) - (a.lastActivity || 0);
  });
  
  // Filter out completed unless --all
  const displayStatuses = showAll 
    ? statuses 
    : statuses.filter(s => s.status !== "completed");
  
  if (displayStatuses.length === 0) {
    console.log("No active research sessions.");
    console.log("Use --all to see completed sessions.");
    return;
  }
  
  // Summary stats
  const running = statuses.filter(s => s.status === "running" || s.status === "spawned").length;
  const completed = statuses.filter(s => s.status === "completed").length;
  const totalLearnings = statuses.reduce((sum, s) => sum + s.learningsCount, 0);
  
  console.log(`=== Research Status Summary ===`);
  console.log(`Active: ${running} | Completed: ${completed} | Total Learnings: ${totalLearnings}\n`);
  
  // Display each session
  for (const status of displayStatuses) {
    const shortId = status.sessionId.slice(-8);
    const title = status.title.length > 40 ? status.title.slice(0, 37) + "..." : status.title;
    const statusEmoji = status.status === "completed" ? "✓" : 
                       status.status === "running" ? "▶" : 
                       status.status === "error" ? "✗" : "?";
    
    console.log(`${statusEmoji} ${shortId}  ${status.status.toUpperCase().padEnd(10)}  ${title}`);
    console.log(`   Learnings: ${status.learningsCount} | Last activity: ${formatTimeAgo(status.lastActivity)}`);
    
    if (status.agent) {
      console.log(`   Agent: ${status.agent}${status.tier ? ` (${status.tier})` : ""}`);
    }
    
    if (status.createdAt) {
      const duration = status.lastActivity 
        ? formatDuration(status.lastActivity - status.createdAt)
        : "unknown";
      console.log(`   Duration: ${duration}`);
    }
    
    if (showLearnings && status.learnings.length > 0) {
      console.log("   Recent learnings:");
      for (const learning of status.learnings) {
        const desc = learning.description.length > 60 
          ? learning.description.slice(0, 57) + "..." 
          : learning.description;
        console.log(`     [${learning.category}] ${desc}`);
      }
    }
    
    console.log();
  }
  
  // Commands help
  console.log("Commands:");
  console.log(`  just actorcode messages --id <session_id> --role assistant --latest`);
  console.log(`  just actorcode logs --id <session_id>`);
  if (!showAll) {
    console.log(`  just research-status --all    # Show completed sessions`);
  }
  if (!showLearnings) {
    console.log(`  just research-status --learnings  # Show recent learnings`);
  }
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
