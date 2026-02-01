import fs from "fs/promises";
import path from "path";

const LOG_DIR = path.join(process.cwd(), "logs", "actorcode");

export async function ensureLogDir() {
  await fs.mkdir(LOG_DIR, { recursive: true });
}

export function supervisorLogPath() {
  return path.join(LOG_DIR, "supervisor.log");
}

export function sessionLogPath(sessionId) {
  return path.join(LOG_DIR, `${sessionId}.log`);
}

function formatLogLine(scope, message) {
  const timestamp = new Date().toISOString();
  return `${timestamp} [${scope}] ${message}`;
}

export async function logSupervisor(message) {
  await ensureLogDir();
  await fs.appendFile(supervisorLogPath(), `${formatLogLine("supervisor", message)}\n`);
}

export async function logSession(sessionId, message) {
  if (!sessionId) {
    return;
  }

  await ensureLogDir();
  await fs.appendFile(sessionLogPath(sessionId), `${formatLogLine("session", message)}\n`);
}
