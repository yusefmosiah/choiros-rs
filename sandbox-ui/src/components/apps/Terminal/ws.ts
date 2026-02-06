export type TerminalWsMessage =
  | { type: 'input'; data: string }
  | { type: 'output'; data: string }
  | { type: 'resize'; rows: number; cols: number }
  | { type: 'info'; terminal_id: string; is_running: boolean }
  | { type: 'error'; message: string };

export function parseTerminalWsMessage(raw: string): TerminalWsMessage | null {
  try {
    const parsed = JSON.parse(raw) as { type?: unknown };
    if (!parsed || typeof parsed !== 'object' || typeof parsed.type !== 'string') {
      return null;
    }

    return parsed as TerminalWsMessage;
  } catch {
    return null;
  }
}

export function reconnectDelayMs(
  attempt: number,
  baseMs: number = 500,
  maxMs: number = 8_000,
): number {
  const safeAttempt = Math.max(0, attempt);
  return Math.min(maxMs, baseMs * 2 ** safeAttempt);
}
