const DEFAULT_MODEL = "glm-4.7-flash";
const DEFAULT_TIMEOUT_MS = 60000;
const DEFAULT_MAX_CHARS = 160000;

function getSummaryConfig(overrides = {}) {
  const baseUrl =
    overrides.baseUrl ||
    process.env.ACTORCODE_SUMMARY_BASE_URL ||
    process.env.ZAI_API_BASE_URL ||
    "";
  const endpointOverride =
    overrides.endpoint ||
    process.env.ACTORCODE_SUMMARY_ENDPOINT ||
    "";
  const pathOverride =
    overrides.path ||
    process.env.ACTORCODE_SUMMARY_PATH ||
    "";
  const apiKey =
    overrides.apiKey ||
    process.env.ACTORCODE_SUMMARY_API_KEY ||
    process.env.ZAI_API_KEY ||
    "";
  const model = overrides.model || process.env.ACTORCODE_SUMMARY_MODEL || DEFAULT_MODEL;
  const timeoutMs = Number(overrides.timeoutMs || DEFAULT_TIMEOUT_MS);
  const maxChars = Number(overrides.maxChars || DEFAULT_MAX_CHARS);

  if (!baseUrl) {
    throw new Error(
      "Missing summary base URL. Set ACTORCODE_SUMMARY_BASE_URL or ZAI_API_BASE_URL."
    );
  }
  if (!apiKey) {
    throw new Error(
      "Missing summary API key. Set ACTORCODE_SUMMARY_API_KEY or ZAI_API_KEY."
    );
  }

  return {
    baseUrl,
    endpointOverride,
    pathOverride,
    apiKey,
    model,
    timeoutMs,
    maxChars
  };
}

function normalizeUrl(url) {
  if (!url) return "";
  return url.endsWith("/") ? url.slice(0, -1) : url;
}

function resolveEndpoint({ baseUrl, endpointOverride, pathOverride }) {
  if (endpointOverride) {
    return endpointOverride;
  }

  const normalizedBase = normalizeUrl(baseUrl);
  if (!normalizedBase) {
    return "";
  }

  const lower = normalizedBase.toLowerCase();
  if (
    lower.includes("/chat/completions") ||
    lower.includes("/responses") ||
    lower.includes("/completions")
  ) {
    return normalizedBase;
  }

  if (pathOverride) {
    const normalizedPath = pathOverride.startsWith("/") ? pathOverride : `/${pathOverride}`;
    return `${normalizedBase}${normalizedPath}`;
  }

  if (lower.includes("/api/paas/") || lower.endsWith("/v4")) {
    return `${normalizedBase}/chat/completions`;
  }

  if (lower.endsWith("/v1")) {
    return `${normalizedBase}/chat/completions`;
  }

  return `${normalizedBase}/v1/chat/completions`;
}

function messageRole(message) {
  return message?.info?.role || message?.role || "unknown";
}

function messageId(message) {
  return message?.info?.id || message?.id || "unknown";
}

function messageTime(message) {
  const created = message?.info?.time?.created || message?.time?.created || null;
  if (!created) return "unknown";
  return new Date(created).toISOString();
}

function messageText(message) {
  const parts = message?.parts || [];
  const text = parts
    .filter((part) => part?.type === "text" && part?.text)
    .map((part) => part.text)
    .join("\n");

  if (text.trim()) {
    return text;
  }

  if (parts.length === 0) {
    return "[no parts]";
  }

  const types = parts.map((part) => part?.type || "unknown").join(", ");
  return `[no text parts: ${types}]`;
}

export function buildTranscript(messages) {
  if (!Array.isArray(messages) || messages.length === 0) {
    return "";
  }

  return messages
    .map((message) => {
      const role = messageRole(message).toUpperCase();
      const id = messageId(message);
      const time = messageTime(message);
      const text = messageText(message);
      return `### ${role} ${id} ${time}\n${text}`;
    })
    .join("\n\n");
}

async function callSummaryModel({ endpoint, apiKey, model, timeoutMs, prompt }) {
  if (!globalThis.fetch) {
    throw new Error("fetch() not available. Use Node.js 18+.");
  }

  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMs);

  try {
    const response = await fetch(endpoint, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${apiKey}`
      },
      body: JSON.stringify({
        model,
        messages: [
          {
            role: "system",
            content:
              "You summarize run logs into a single Markdown document. Be concise and structured."
          },
          { role: "user", content: prompt }
        ]
      }),
      signal: controller.signal
    });

    const payload = await response.json().catch(() => ({}));
    if (!response.ok) {
      const detail = payload?.error?.message || response.statusText || "request failed";
      throw new Error(`Summary request failed (${response.status}): ${detail}`);
    }

    const content =
      payload?.choices?.[0]?.message?.content ||
      payload?.choices?.[0]?.text ||
      payload?.output?.[0]?.content ||
      "";

    if (!content.trim()) {
      throw new Error("Summary response was empty.");
    }

    return content;
  } finally {
    clearTimeout(timeout);
  }
}

export async function summarizeMessages(messages, options = {}) {
  const { baseUrl, endpointOverride, pathOverride, apiKey, model, timeoutMs, maxChars } =
    getSummaryConfig(options);
  const endpoint = resolveEndpoint({ baseUrl, endpointOverride, pathOverride });
  if (!endpoint) {
    throw new Error("Summary endpoint could not be resolved. Check base URL settings.");
  }
  const transcript = buildTranscript(messages);

  if (!transcript.trim()) {
    return {
      markdown: "No messages found for this session.",
      model,
      truncated: false
    };
  }

  let truncated = false;
  let promptTranscript = transcript;
  if (transcript.length > maxChars) {
    truncated = true;
    promptTranscript = transcript.slice(transcript.length - maxChars);
  }

  const headerNote = truncated
    ? `Note: transcript truncated to last ${maxChars} characters.\n\n`
    : "";

  const prompt = `${headerNote}Summarize the following run log as a Markdown document with these sections:\n\n` +
    `# Run Summary\n` +
    `## Key Findings\n` +
    `## Decisions/Actions\n` +
    `## Evidence/References\n` +
    `## Blockers/Risks\n` +
    `## Next Steps\n\n` +
    `Run log:\n${promptTranscript}`;

  const markdown = await callSummaryModel({ endpoint, apiKey, model, timeoutMs, prompt });
  return { markdown, model, truncated };
}
