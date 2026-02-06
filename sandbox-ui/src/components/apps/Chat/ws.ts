import { httpToWsUrl } from '@/lib/ws/client';

export type ChatStreamMessage =
  | { type: 'connected'; actor_id: string; user_id: string }
  | { type: 'thinking'; content: string }
  | { type: 'tool_call'; content: string }
  | { type: 'tool_result'; content: string }
  | { type: 'response'; content: string }
  | { type: 'error'; message: string }
  | { type: 'pong' };

export interface ChatResponsePayload {
  text: string;
  confidence?: number;
  model_used?: string;
  client_message_id?: string;
}

export function parseChatStreamMessage(raw: string): ChatStreamMessage | null {
  try {
    const parsed = JSON.parse(raw) as { type?: unknown; content?: unknown; message?: unknown };
    if (!parsed || typeof parsed !== 'object') {
      return null;
    }

    if (typeof parsed.type !== 'string') {
      return null;
    }

    if (
      (parsed.type === 'thinking' ||
        parsed.type === 'tool_call' ||
        parsed.type === 'tool_result' ||
        parsed.type === 'response') &&
      typeof parsed.content !== 'string'
    ) {
      return null;
    }

    if (parsed.type === 'error' && typeof parsed.message !== 'string') {
      return null;
    }

    return parsed as ChatStreamMessage;
  } catch {
    return null;
  }
}

export function parseResponsePayload(content: string): ChatResponsePayload {
  try {
    const parsed = JSON.parse(content) as {
      text?: unknown;
      confidence?: unknown;
      model_used?: unknown;
      client_message_id?: unknown;
    };

    const text = typeof parsed.text === 'string' ? parsed.text : content;
    const confidence =
      typeof parsed.confidence === 'number' ? parsed.confidence : undefined;
    const modelUsed =
      typeof parsed.model_used === 'string' ? parsed.model_used : undefined;
    const clientMessageId =
      typeof parsed.client_message_id === 'string' ? parsed.client_message_id : undefined;

    return {
      text,
      confidence,
      model_used: modelUsed,
      client_message_id: clientMessageId,
    };
  } catch {
    return { text: content };
  }
}

export function buildChatWebSocketUrl(actorId: string, userId: string): string {
  const baseUrl = import.meta.env.VITE_API_URL || 'http://localhost:8080';
  const wsBase = httpToWsUrl(baseUrl);
  return `${wsBase}/ws/chat/${encodeURIComponent(actorId)}/${encodeURIComponent(userId)}`;
}
