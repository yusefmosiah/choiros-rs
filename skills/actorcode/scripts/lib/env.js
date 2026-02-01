import fs from "fs/promises";
import path from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

function parseEnvLine(line) {
  const trimmed = line.trim();
  if (!trimmed || trimmed.startsWith("#")) {
    return null;
  }
  const eqIndex = trimmed.indexOf("=");
  if (eqIndex === -1) {
    return null;
  }
  const key = trimmed.slice(0, eqIndex).trim();
  let value = trimmed.slice(eqIndex + 1).trim();
  if (!key) {
    return null;
  }
  if (
    (value.startsWith("\"") && value.endsWith("\"")) ||
    (value.startsWith("'") && value.endsWith("'"))
  ) {
    value = value.slice(1, -1);
  }
  return { key, value };
}

async function fileExists(filePath) {
  try {
    await fs.access(filePath);
    return true;
  } catch {
    return false;
  }
}

export async function loadEnvFile(customPath = null) {
  const repoRoot = path.resolve(__dirname, "../../../..");
  const candidates = [
    customPath,
    process.env.ACTORCODE_ENV_PATH,
    path.join(process.cwd(), ".env"),
    path.join(repoRoot, ".env")
  ].filter(Boolean);

  let envPath = null;
  for (const candidate of candidates) {
    if (await fileExists(candidate)) {
      envPath = candidate;
      break;
    }
  }

  if (!envPath) {
    return { loaded: false, path: null };
  }

  const raw = await fs.readFile(envPath, "utf8");
  const lines = raw.split("\n");
  for (const line of lines) {
    const parsed = parseEnvLine(line);
    if (!parsed) {
      continue;
    }
    if (!process.env[parsed.key]) {
      process.env[parsed.key] = parsed.value;
    }
  }

  return { loaded: true, path: envPath };
}
