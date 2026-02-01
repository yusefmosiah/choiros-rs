#!/usr/bin/env node
import fs from "fs/promises";
import path from "path";
import { spawn } from "child_process";
import { createClient, getServerConfig } from "./lib/client.js";
import { parseArgs } from "./lib/args.js";
import {
  logSession,
  logSupervisor,
  sessionLogPath,
  supervisorLogPath
} from "./lib/logs.js";
import {
  loadRegistry,
  updateSessionRegistry
} from "./lib/registry.js";

const DIRECTORY = process.cwd();
const ALLOWED_MODELS = [
  "zai-coding-plan/glm-4.7-flash",
  "zai-coding-plan/glm-4.7",
  "opencode/kimi-k2.5-free",
  "openai/gpt-5.2-codex"
];
const MODEL_TIERS = {
  pico: "zai-coding-plan/glm-4.7-flash",
  nano: "zai-coding-plan/glm-4.7",
  micro: "opencode/kimi-k2.5-free",
  milli: "openai/gpt-5.2-codex"
};
const MODEL_DESCRIPTIONS = {
  pico: "Text-only. Run scripts/tools and quick research; not for writing new code.",
  nano: "Text-only. Coding-capable worker for straightforward changes.",
  micro: "Multimodal (text+image). General-purpose, resource-efficient default.",
  milli: "Multimodal (text+image). Long-context + debugging heavy lifting."
};
const DEFAULT_MODEL = MODEL_TIERS.pico;

function parseModel(model, tier) {
  if (model && tier) {
    throw new Error("Use either --model or --tier, not both.");
  }

  if (tier) {
    const resolved = MODEL_TIERS[tier];
    if (!resolved) {
      throw new Error(`Unknown tier. Use one of: ${Object.keys(MODEL_TIERS).join(", ")}`);
    }
    return { model: resolved, parsed: toModelParts(resolved), tier };
  }

  if (!model) {
    return { model: DEFAULT_MODEL, parsed: toModelParts(DEFAULT_MODEL), tier: "pico" };
  }

  if (!ALLOWED_MODELS.includes(model)) {
    throw new Error(`Model not allowed. Use one of: ${ALLOWED_MODELS.join(", ")}`);
  }
  const matchedTier = Object.keys(MODEL_TIERS).find((key) => MODEL_TIERS[key] === model) || null;
  return { model, parsed: toModelParts(model), tier: matchedTier };
}

function toModelParts(model) {
  const [providerID, modelID] = model.split("/");
  if (!providerID || !modelID) {
    throw new Error("Model must be in provider/model format.");
  }
  return { providerID, modelID };
}

function toTextParts(text) {
  return [{ type: "text", text }];
}

function messageRole(message) {
  return message?.info?.role || message?.role || "unknown";
}

function messageId(message) {
  return message?.info?.id || message?.id || "unknown";
}

function messageTime(message) {
  const created = message?.info?.time?.created || message?.time?.created || null;
  if (!created) {
    return "unknown";
  }
  return new Date(created).toISOString();
}

function messageText(message) {
  const parts = message?.parts || [];
  return parts
    .filter((part) => part?.type === "text" && part?.text)
    .map((part) => part.text)
    .join("\n");
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function unwrap(response, label) {
  if (response?.error) {
    const message = response.error?.name || "request_failed";
    throw new Error(`${label} failed: ${message}`);
  }
  return response?.data;
}

function usage() {
  return [
    "actorcode spawn --title <title> --agent <agent> --model <provider/model> --tier <pico|nano|micro|milli> --prompt <text>",
    "actorcode status",
    "actorcode models",
    "actorcode message --to <session_id> --text <message>",
    "actorcode messages --id <session_id> [--limit 20] [--role assistant|user|any] [--latest] [--wait] [--interval 1000] [--timeout 120]",
    "actorcode abort --id <session_id>",
    "actorcode events [--session <session_id>]",
    "actorcode logs [--id <session_id>]",
    "actorcode attach -- <args>"
  ].join("\n");
}

async function handleSpawn(options) {
  const title = options.title || "actorcode-session";
  const prompt = options.prompt;
  const agent = options.agent;
  const { model: modelName, parsed: model, tier } = parseModel(options.model, options.tier);

  if (!prompt) {
    throw new Error("spawn requires --prompt");
  }

  const client = createClient();
  const sessionResponse = await client.session.create({
    query: { directory: DIRECTORY },
    body: { title }
  });
  const session = unwrap(sessionResponse, "create session");
  const sessionId = session.id;

  await updateSessionRegistry(sessionId, {
    title,
    agent: agent || null,
    model: modelName || null,
    tier: tier || null,
    createdAt: Date.now(),
    status: "spawned",
    lastEventAt: null
  });

  await logSupervisor(`spawn session=${sessionId} title=${title} model=${modelName} tier=${tier || ""}`);
  await logSession(sessionId, `spawned title=${title} agent=${agent || ""} model=${modelName} tier=${tier || ""}`);

  const promptBody = {
    parts: toTextParts(prompt)
  };
  if (agent) {
    promptBody.agent = agent;
  }
  if (model) {
    promptBody.model = model;
  }

  await client.session.promptAsync({
    path: { id: sessionId },
    query: { directory: DIRECTORY },
    body: promptBody
  });

  await logSession(sessionId, "prompt_async dispatched");
  process.stdout.write(`${sessionId}\n`);
}

async function handleStatus() {
  const client = createClient();
  const listResponse = await client.session.list({ query: { directory: DIRECTORY } });
  const statusResponse = await client.session.status({ query: { directory: DIRECTORY } });

  const sessions = unwrap(listResponse, "list sessions") || [];
  const statuses = unwrap(statusResponse, "session status") || {};

  await logSupervisor(`status sessions=${sessions.length}`);

  for (const session of sessions) {
    const status = statuses[session.id];
    await updateSessionRegistry(session.id, {
      title: session.title,
      status: status?.type || "unknown",
      lastEventAt: session.time?.updated || null
    });
  }

  const lines = sessions.map((session) => {
    const status = statuses[session.id];
    const statusText = status?.type || "unknown";
    return `${session.id}  ${statusText}  ${session.title}`;
  });

  process.stdout.write(`${lines.join("\n")}\n`);
}

async function handleModels() {
  const rows = Object.keys(MODEL_TIERS).map((tier) => {
    const model = MODEL_TIERS[tier];
    const description = MODEL_DESCRIPTIONS[tier] || "";
    return `${tier}  ${model}  ${description}`;
  });

  process.stdout.write(`${rows.join("\n")}\n`);
}

async function handleMessage(options) {
  const sessionId = options.to || options.id;
  const text = options.text || options.prompt;

  if (!sessionId || !text) {
    throw new Error("message requires --to <session_id> and --text");
  }

  const client = createClient();
  await client.session.promptAsync({
    path: { id: sessionId },
    query: { directory: DIRECTORY },
    body: {
      parts: toTextParts(text)
    }
  });

  const messagesResponse = await client.session.messages({
    path: { id: sessionId },
    query: { directory: DIRECTORY, limit: 1 }
  });
  const messages = unwrap(messagesResponse, "list messages") || [];
  if (messages[0]) {
    await logSession(sessionId, `message.latest role=${messages[0].role} id=${messages[0].id}`);
  }

  await logSupervisor(`message session=${sessionId}`);
  await logSession(sessionId, `message ${text}`);
  process.stdout.write(`${sessionId}\n`);
}

async function handleMessages(options) {
  const sessionId = options.id || options.session || options.to;
  const limit = Number(options.limit || 20);
  const roleFilter = (options.role || "any").toLowerCase();
  const latestOnly = Boolean(options.latest);
  const wait = Boolean(options.wait);
  const intervalMs = Number(options.interval || 1000);
  const timeoutSec = options.timeout ? Number(options.timeout) : null;

  if (!sessionId) {
    throw new Error("messages requires --id <session_id>");
  }

  const client = createClient();
  const startTime = Date.now();

  const fetchMessages = async () => {
    const messagesResponse = await client.session.messages({
      path: { id: sessionId },
      query: { directory: DIRECTORY, limit }
    });
    const messages = unwrap(messagesResponse, "list messages") || [];
    const filtered = messages.filter((message) => {
      if (roleFilter === "any") {
        return true;
      }
      return messageRole(message).toLowerCase() === roleFilter;
    });
    return filtered;
  };

  while (true) {
    const filtered = await fetchMessages();
    if (filtered.length > 0) {
      const output = [];
      const list = latestOnly ? [filtered[0]] : filtered;
      for (const message of list) {
        const header = `${messageRole(message)} ${messageId(message)} ${messageTime(message)}`;
        output.push(header);
        const text = messageText(message);
        output.push(text ? text : "(no text parts)");
        output.push("");
      }
      process.stdout.write(`${output.join("\n")}\n`);
      return;
    }

    if (!wait) {
      process.stdout.write("(no matching messages)\n");
      return;
    }

    if (timeoutSec && Date.now() - startTime > timeoutSec * 1000) {
      throw new Error("messages wait timeout");
    }

    await sleep(intervalMs);
  }
}

async function handleAbort(options) {
  const sessionId = options.id;
  if (!sessionId) {
    throw new Error("abort requires --id <session_id>");
  }

  const client = createClient();
  await client.session.abort({ path: { id: sessionId }, query: { directory: DIRECTORY } });
  await updateSessionRegistry(sessionId, { status: "aborted" });
  await logSupervisor(`abort session=${sessionId}`);
  await logSession(sessionId, "abort requested");
  process.stdout.write(`${sessionId}\n`);
}

function extractSessionId(event) {
  const props = event?.properties || {};
  if (props.sessionID) {
    return props.sessionID;
  }
  if (props.info?.sessionID) {
    return props.info.sessionID;
  }
  if (props.info?.id) {
    return props.info.id;
  }
  if (props.part?.sessionID) {
    return props.part.sessionID;
  }
  return null;
}

function describeEvent(event) {
  const type = event?.type || "event";
  const props = event?.properties || {};

  if (type === "message.updated" && props.info) {
    return `message.updated role=${props.info.role} id=${props.info.id}`;
  }

  if (type === "message.part.updated" && props.part) {
    if (props.part.type === "tool") {
      return `tool ${props.part.tool} status=${props.part.state?.status || "unknown"}`;
    }
    return `message.part.updated type=${props.part.type}`;
  }

  if (type === "session.status" && props.status) {
    return `session.status ${props.status.type}`;
  }

  return type;
}

async function handleEvents(options) {
  const client = createClient();
  const onlySession = options.session || options.id || null;

  await logSupervisor("events subscribe");

  const sse = await client.event.subscribe({ query: { directory: DIRECTORY } });
  for await (const event of sse.stream) {
    const sessionId = extractSessionId(event);
    if (onlySession && sessionId !== onlySession) {
      continue;
    }

    const description = describeEvent(event);
    await logSupervisor(`event ${description}`);
    if (sessionId) {
      await logSession(sessionId, description);
      await updateSessionRegistry(sessionId, { lastEventAt: Date.now() });
    }

    process.stdout.write(`${description}\n`);
  }
}

async function handleLogs(options) {
  const sessionId = options.id || options.session || null;
  const logPath = sessionId ? sessionLogPath(sessionId) : supervisorLogPath();

  await logSupervisor(`logs tail path=${logPath}`);

  let content = "";
  try {
    content = await fs.readFile(logPath, "utf8");
  } catch (error) {
    if (error && error.code === "ENOENT") {
      throw new Error(`Log file not found: ${logPath}`);
    }
    throw error;
  }

  const lines = content.split("\n");
  const tail = lines.slice(Math.max(0, lines.length - 200)).join("\n");
  if (tail.trim()) {
    process.stdout.write(`${tail}\n`);
  }

  let lastSize = Buffer.byteLength(content);
  const watcher = (await import("fs")).watch(logPath, async (eventType) => {
    if (eventType !== "change") {
      return;
    }
    const stats = await fs.stat(logPath);
    if (stats.size <= lastSize) {
      return;
    }
    const stream = (await import("fs")).createReadStream(logPath, {
      start: lastSize,
      end: stats.size
    });
    stream.on("data", (chunk) => {
      process.stdout.write(chunk.toString("utf8"));
    });
    lastSize = stats.size;
  });

  process.on("SIGINT", () => {
    watcher.close();
    process.exit(0);
  });
}

async function handleAttach(rest) {
  const { baseUrl } = getServerConfig();
  await logSupervisor("attach" );
  const child = spawn("opencode", ["attach", ...rest], {
    stdio: "inherit",
    env: {
      ...process.env,
      OPENCODE_SERVER_URL: baseUrl
    }
  });

  child.on("exit", (code) => {
    process.exit(code ?? 0);
  });
}

async function main() {
  const { args, options, rest } = parseArgs(process.argv.slice(2));
  const command = args[0];

  if (!command || options.help) {
    process.stdout.write(`${usage()}\n`);
    return;
  }

  await loadRegistry();
  await logSupervisor(`command ${command}`);

  switch (command) {
    case "spawn":
      await handleSpawn(options);
      return;
    case "status":
      await handleStatus();
      return;
    case "models":
      await handleModels();
      return;
    case "message":
      await handleMessage(options);
      return;
    case "messages":
      await handleMessages(options);
      return;
    case "abort":
      await handleAbort(options);
      return;
    case "events":
      await handleEvents(options);
      return;
    case "logs":
      await handleLogs(options);
      return;
    case "attach":
      await handleAttach(rest);
      return;
    default:
      throw new Error(`Unknown command: ${command}`);
  }
}

main().catch(async (error) => {
  await logSupervisor(`error ${error.message}`);
  process.stderr.write(`${error.message}\n`);
  process.exit(1);
});
