import fs from "fs/promises";
import path from "path";

const REGISTRY_DIR = path.join(process.cwd(), ".actorcode");
const REGISTRY_PATH = path.join(REGISTRY_DIR, "registry.json");

const EMPTY_REGISTRY = {
  version: 1,
  updatedAt: 0,
  sessions: {}
};

export function registryPath() {
  return REGISTRY_PATH;
}

export async function loadRegistry() {
  try {
    const raw = await fs.readFile(REGISTRY_PATH, "utf8");
    const parsed = JSON.parse(raw);
    return {
      ...EMPTY_REGISTRY,
      ...parsed,
      sessions: { ...EMPTY_REGISTRY.sessions, ...(parsed.sessions || {}) }
    };
  } catch (error) {
    if (error && error.code === "ENOENT") {
      return { ...EMPTY_REGISTRY };
    }
    throw error;
  }
}

export async function saveRegistry(registry) {
  await fs.mkdir(REGISTRY_DIR, { recursive: true });
  await fs.writeFile(REGISTRY_PATH, `${JSON.stringify(registry, null, 2)}\n`, "utf8");
}

export async function updateSessionRegistry(sessionId, patch) {
  const registry = await loadRegistry();
  const current = registry.sessions[sessionId] || {};
  registry.sessions[sessionId] = { ...current, ...patch };
  registry.updatedAt = Date.now();
  await saveRegistry(registry);
  return registry.sessions[sessionId];
}
