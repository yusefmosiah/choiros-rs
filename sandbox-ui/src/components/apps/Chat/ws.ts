import { httpToWsUrl } from '@/lib/ws/client';

export type ChatStreamMessage =
  | { type: 'connected'; actor_id: string; user_id: string }
  | { type: 'thinking'; content: string }
  | { type: 'tool_call'; content: string }
  | { type: 'tool_result'; content: string }
  | { type: 'response'; content: string }
  | { type: 'error'; message: string }
  | { type: 'pong' };

export function parseChatStreamMessage(raw: string): ChatStreamMessage | null {
  try {
    const parsed = JSON.parse(raw) as { type?: unknown };
    if (!parsed || typeof parsed !== 'object' || typeof parsed.type !== 'string') {
      return null;
    }

    return parsed as ChatStreamMessage;
  } catch {
    return null;
  }
}

export function parseResponseText(content: string): string {
  try {
    const parsed = JSON.parse(content) as { text?: unknown };
    return typeof parsed.text === 'string' ? parsed.text : content;
  } catch {
    return content;
  }
}

export function buildChatWebSocketUrl(actorId: string, userId: string): string {
  const baseUrl = import.meta.env.VITE_API_URL || 'http://localhost:8080';
  const wsBase = httpToWsUrl(baseUrl);
  return `${wsBase}/ws/chat/${encodeURIComponent(actorId)}/${encodeURIComponent(userId)}`;
}
