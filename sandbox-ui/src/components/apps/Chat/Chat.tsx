import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { getMessages, sendMessage } from '@/lib/api/chat';
import type { ChatMessage } from '@/types/generated';
import { useChatStore } from '@/stores/chat';
import { buildChatWebSocketUrl, parseChatStreamMessage, parseResponsePayload } from './ws';
import './Chat.css';

interface ChatProps {
  actorId: string;
  userId?: string;
}

type StreamEventType = 'thinking' | 'tool_call' | 'tool_result';

interface StreamEvent {
  id: string;
  type: StreamEventType;
  text: string;
  timestamp: string;
}

function sortMessages(messages: ChatMessage[]): ChatMessage[] {
  return [...messages].sort((a, b) => a.timestamp.localeCompare(b.timestamp));
}

function parseStreamContent(content: string): string {
  try {
    const parsed = JSON.parse(content) as unknown;
    if (typeof parsed === 'string') {
      return parsed;
    }
    return JSON.stringify(parsed, null, 2);
  } catch {
    return content;
  }
}

export function Chat({ actorId, userId = 'user-1' }: ChatProps) {
  const messages = useChatStore((state) => state.messages);
  const setMessages = useChatStore((state) => state.setMessages);
  const addMessage = useChatStore((state) => state.addMessage);
  const updatePendingMessage = useChatStore((state) => state.updatePendingMessage);
  const isLoading = useChatStore((state) => state.isLoading);
  const setLoading = useChatStore((state) => state.setLoading);
  const error = useChatStore((state) => state.error);
  const setError = useChatStore((state) => state.setError);

  const [draft, setDraft] = useState('');
  const [connected, setConnected] = useState(false);
  const [streamEvents, setStreamEvents] = useState<StreamEvent[]>([]);
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const reconnectAttemptsRef = useRef(0);
  const pendingQueueRef = useRef<string[]>([]);
  const pendingTimeoutsRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());

  const clearPendingTimeout = useCallback((messageId: string) => {
    const timeout = pendingTimeoutsRef.current.get(messageId);
    if (timeout) {
      clearTimeout(timeout);
      pendingTimeoutsRef.current.delete(messageId);
    }
  }, []);

  const startPendingTimeout = useCallback(
    (messageId: string) => {
      clearPendingTimeout(messageId);
      const timeout = setTimeout(() => {
        updatePendingMessage(messageId, false);
        setError('Assistant response timeout. You can retry or refresh.');
        pendingQueueRef.current = pendingQueueRef.current.filter((id) => id !== messageId);
        pendingTimeoutsRef.current.delete(messageId);
      }, 20_000);
      pendingTimeoutsRef.current.set(messageId, timeout);
    },
    [clearPendingTimeout, setError, updatePendingMessage],
  );

  const loadMessages = useCallback(async () => {
    try {
      setLoading(true);
      const data = await getMessages(actorId);
      setMessages(sortMessages(data));
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load chat messages');
    } finally {
      setLoading(false);
    }
  }, [actorId, setError, setLoading, setMessages]);

  useEffect(() => {
    let cancelled = false;

    const clearReconnect = () => {
      if (reconnectTimerRef.current) {
        clearTimeout(reconnectTimerRef.current);
        reconnectTimerRef.current = null;
      }
    };

    const scheduleReconnect = () => {
      clearReconnect();
      const attempt = reconnectAttemptsRef.current;
      const delay = Math.min(8000, 500 * 2 ** attempt);
      reconnectAttemptsRef.current += 1;

      reconnectTimerRef.current = setTimeout(() => {
        if (!cancelled) {
          connect();
        }
      }, delay);
    };

    const connect = () => {
      const ws = new WebSocket(buildChatWebSocketUrl(actorId, userId));
      wsRef.current = ws;

      ws.onopen = () => {
        if (cancelled) {
          return;
        }

        reconnectAttemptsRef.current = 0;
        setConnected(true);
        setError(null);
      };

      ws.onmessage = (event) => {
        if (typeof event.data !== 'string') {
          return;
        }

        const message = parseChatStreamMessage(event.data);
        if (!message) {
          return;
        }

        if (message.type === 'response') {
          const payload = parseResponsePayload(message.content);
          let pendingId = payload.client_message_id;

          if (pendingId && !pendingQueueRef.current.includes(pendingId)) {
            pendingId = undefined;
          }
          if (!pendingId) {
            pendingId = pendingQueueRef.current.shift();
          } else {
            pendingQueueRef.current = pendingQueueRef.current.filter((id) => id !== pendingId);
          }

          if (pendingId) {
            clearPendingTimeout(pendingId);
            updatePendingMessage(pendingId, false);
          }

          addMessage({
            id: `assistant-${Date.now()}`,
            text: payload.text,
            sender: 'Assistant',
            timestamp: new Date().toISOString(),
            pending: false,
          });
          return;
        }

        if (
          message.type === 'thinking' ||
          message.type === 'tool_call' ||
          message.type === 'tool_result'
        ) {
          setStreamEvents((prev) => {
            const next = [
              ...prev,
              {
                id: `${message.type}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
                type: message.type,
                text: parseStreamContent(message.content),
                timestamp: new Date().toISOString(),
              },
            ];
            return next.slice(-24);
          });
          return;
        }

        if (message.type === 'error') {
          setError(message.message);
        }
      };

      ws.onerror = () => {
        if (!cancelled) {
          setConnected(false);
        }
      };

      ws.onclose = () => {
        if (!cancelled) {
          setConnected(false);
          scheduleReconnect();
        }
      };
    };

    void loadMessages();
    connect();

    return () => {
      cancelled = true;
      clearReconnect();
      for (const timeout of pendingTimeoutsRef.current.values()) {
        clearTimeout(timeout);
      }
      pendingTimeoutsRef.current.clear();
      if (wsRef.current) {
        wsRef.current.close();
        wsRef.current = null;
      }
    };
  }, [actorId, addMessage, clearPendingTimeout, loadMessages, setError, updatePendingMessage, userId]);

  const handleSend = useCallback(async () => {
    const text = draft.trim();
    if (!text) {
      return;
    }

    const tempId = `pending-${Date.now()}`;
    addMessage({
      id: tempId,
      text,
      sender: 'User',
      timestamp: new Date().toISOString(),
      pending: true,
    });

    pendingQueueRef.current.push(tempId);
    setStreamEvents([]);
    setDraft('');

    const ws = wsRef.current;
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(
        JSON.stringify({
          type: 'message',
          text,
          client_message_id: tempId,
        }),
      );
      startPendingTimeout(tempId);
      return;
    }

    try {
      await sendMessage(actorId, {
        text,
        user_id: userId,
      });

      clearPendingTimeout(tempId);
      updatePendingMessage(tempId, false);
      setError(null);
      setTimeout(() => {
        void loadMessages();
      }, 500);
    } catch (err) {
      clearPendingTimeout(tempId);
      setError(err instanceof Error ? err.message : 'Failed to send message');
    }
  }, [
    actorId,
    addMessage,
    clearPendingTimeout,
    draft,
    loadMessages,
    setError,
    startPendingTimeout,
    updatePendingMessage,
    userId,
  ]);

  const renderedMessages = useMemo(() => sortMessages(messages), [messages]);

  return (
    <div className="chat-app">
      <header className="chat-app__header">
        <h3>Chat</h3>
        <div className="chat-app__header-right">
          <span className={`chat-app__socket ${connected ? 'chat-app__socket--ok' : ''}`}>
            {connected ? 'Live' : 'Retrying'}
          </span>
          <button type="button" onClick={() => void loadMessages()} disabled={isLoading}>
            Refresh
          </button>
        </div>
      </header>

      <div className="chat-app__messages">
        {streamEvents.length > 0 && (
          <section className="chat-stream" aria-live="polite">
            <h4 className="chat-stream__title">Live activity</h4>
            <div className="chat-stream__list">
              {streamEvents.map((event) => (
                <article key={event.id} className={`chat-stream__event chat-stream__event--${event.type}`}>
                  <div className="chat-stream__meta">
                    <span>{event.type.replace('_', ' ')}</span>
                    <span>{new Date(event.timestamp).toLocaleTimeString()}</span>
                  </div>
                  <pre>{event.text}</pre>
                </article>
              ))}
            </div>
          </section>
        )}

        {isLoading && renderedMessages.length === 0 && <p className="chat-app__status">Loading messages...</p>}

        {!isLoading && renderedMessages.length === 0 && (
          <p className="chat-app__status">No messages yet. Send something to start.</p>
        )}

        {renderedMessages.map((message) => (
          <article
            key={message.id}
            className={`chat-msg ${message.sender === 'User' ? 'chat-msg--user' : ''} ${message.pending ? 'chat-msg--pending' : ''}`}
          >
            <div className="chat-msg__meta">
              <span>{message.sender}</span>
              <span>{new Date(message.timestamp).toLocaleTimeString()}</span>
            </div>
            <p>{message.text}</p>
          </article>
        ))}
      </div>

      {error && <p className="chat-app__error">{error}</p>}

      <footer className="chat-app__composer">
        <textarea
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && !event.shiftKey) {
              event.preventDefault();
              void handleSend();
            }
          }}
          placeholder="Type a message..."
          rows={2}
        />
        <button type="button" onClick={() => void handleSend()} disabled={draft.trim().length === 0}>
          Send
        </button>
      </footer>
    </div>
  );
}
