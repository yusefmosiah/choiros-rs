import { createOpencodeClient } from "@opencode-ai/sdk";

export function getServerConfig() {
  const baseUrl = process.env.OPENCODE_SERVER_URL || "http://localhost:4096";
  const username = process.env.OPENCODE_SERVER_USERNAME || "opencode";
  const password = process.env.OPENCODE_SERVER_PASSWORD || "";

  let headers;
  if (password) {
    const token = Buffer.from(`${username}:${password}`).toString("base64");
    headers = { Authorization: `Basic ${token}` };
  }

  return { baseUrl, headers };
}

export function createClient() {
  const { baseUrl, headers } = getServerConfig();
  return createOpencodeClient({ baseUrl, headers });
}
