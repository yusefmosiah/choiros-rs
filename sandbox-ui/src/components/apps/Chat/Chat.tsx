import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { getMessages, sendMessage } from '@/lib/api/chat';
import type { ChatMessage } from '@/types/generated';
import { useChatStoreForActor } from '@/stores/chat';
import { useWindowsStore } from '@/stores/windows';
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

interface MessageGroup {
  id: string;
  userMessage?: ChatMessage;
  reasoning: StreamEvent[];
  toolCalls: StreamEvent[];
  toolResults: StreamEvent[];
  assistantMessage?: ChatMessage;
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

function groupMessages(messages: ChatMessage[]): MessageGroup[] {
  const groups: MessageGroup[] = [];
  let currentGroup: MessageGroup | null = null;

  for (const message of messages) {
    if (message.sender === 'User') {
      // Start a new group for user messages
      currentGroup = {
        id: `group-${message.id}`,
        userMessage: message,
        reasoning: [],
        toolCalls: [],
        toolResults: [],
        assistantMessage: undefined,
      };
      groups.push(currentGroup);
    } else if (message.sender === 'Assistant') {
      // Associate assistant message with current group
      if (currentGroup) {
        currentGroup.assistantMessage = message;
      } else {
        // Orphaned assistant message - create new group
        currentGroup = {
          id: `group-${message.id}`,
          reasoning: [],
          toolCalls: [],
          toolResults: [],
          assistantMessage: message,
        };
        groups.push(currentGroup);
      }
    }
  }

  return groups;
}

export function Chat({ actorId, userId = 'user-1' }: ChatProps) {
  const {
    messages,
    setMessages,
    addMessage,
    updatePendingMessage,
    isLoading,
    setLoading,
    error,
    setError,
  } = useChatStoreForActor(actorId);

  const windows = useWindowsStore((state) => state.windows);
  const focusWindow = useWindowsStore((state) => state.focusWindow);

  const [draft, setDraft] = useState('');
  const [connected, setConnected] = useState(false);
  const [streamEvents, setStreamEvents] = useState<StreamEvent[]>([]);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const reconnectAttemptsRef = useRef(0);
  const pendingQueueRef = useRef<string[]>([]);
  const pendingTimeoutsRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());

  // Get all chat windows for the sidebar
  const chatWindows = useMemo(() => {
    return windows.filter((w) => w.app_id === 'chat');
  }, [windows]);

  // Get current window title for display
  const currentWindow = useMemo(() => {
    return windows.find((w) => w.id === actorId);
  }, [windows, actorId]);

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

  const handleThreadClick = useCallback(
    (threadActorId: string) => {
      // Check if this thread is already open in another window
      const existingWindow = chatWindows.find((w) => w.id === threadActorId);

      if (existingWindow && threadActorId !== actorId) {
        // Focus the existing window instead of opening in current
        focusWindow(threadActorId);
      }
      // If it's the current window, do nothing
    },
    [chatWindows, actorId, focusWindow],
  );

  const toggleGroupExpanded = useCallback((groupId: string) => {
    setExpandedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(groupId)) {
        next.delete(groupId);
      } else {
        next.add(groupId);
      }
      return next;
    });
  }, []);

  // Build message groups
  const messageGroups = useMemo(() => {
    const sorted = sortMessages(messages);
    return groupMessages(sorted);
  }, [messages]);

  // Distribute stream events to groups
  const groupsWithStreamEvents = useMemo(() => {
    if (streamEvents.length === 0) {
      return messageGroups.map((group) => ({ ...group, hasStreamEvents: false }));
    }

    // Find the last group without an assistant message
    const lastIncompleteIndex = messageGroups.findIndex(
      (g, i) => !g.assistantMessage && i === messageGroups.length - 1,
    );

    return messageGroups.map((group, index) => {
      if (index === lastIncompleteIndex) {
        return {
          ...group,
          reasoning: streamEvents.filter((e) => e.type === 'thinking'),
          toolCalls: streamEvents.filter((e) => e.type === 'tool_call'),
          toolResults: streamEvents.filter((e) => e.type === 'tool_result'),
          hasStreamEvents: true,
        };
      }
      return { ...group, hasStreamEvents: false };
    });
  }, [messageGroups, streamEvents]);

  // Truncate title for display
  const truncateTitle = (title: string, maxLen = 20) => {
    if (title.length <= maxLen) return title;
    return title.slice(0, maxLen) + '...';
  };

  return (
    <div className="chat-app">
      {/* Collapsible Sidebar */}
      <aside className={`chat-sidebar ${sidebarOpen ? 'chat-sidebar--open' : ''}`}>
        <div className="chat-sidebar__header">
          <span>Threads</span>
          <button
            type="button"
            className="chat-sidebar__close"
            onClick={() => setSidebarOpen(false)}
            aria-label="Close sidebar"
          >
            ◀
          </button>
        </div>
        <div className="chat-sidebar__list">
          {chatWindows.length === 0 && (
            <div className="chat-sidebar__empty">No chat threads</div>
          )}
          {chatWindows.map((window) => (
            <button
              key={window.id}
              type="button"
              className={`chat-sidebar__item ${window.id === actorId ? 'chat-sidebar__item--active' : ''} ${window.id !== actorId ? 'chat-sidebar__item--other' : ''}`}
              onClick={() => handleThreadClick(window.id)}
              title={window.title}
            >
              <span className="chat-sidebar__item-title">{truncateTitle(window.title)}</span>
              {window.id !== actorId && (
                <span className="chat-sidebar__item-indicator">↗</span>
              )}
            </button>
          ))}
        </div>
      </aside>

      {/* Main Chat Area */}
      <div className="chat-main">
        {!sidebarOpen && (
          <button
            type="button"
            className="chat-sidebar__toggle"
            onClick={() => setSidebarOpen(true)}
            aria-label="Open sidebar"
          >
            ▶
          </button>
        )}

        <header className="chat-app__header">
          <div className="chat-app__header-left">
            <h3>Chat</h3>
            {currentWindow && (
              <span className="chat-app__window-title" title={currentWindow.title}>
                {truncateTitle(currentWindow.title, 30)}
              </span>
            )}
          </div>
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
          {isLoading && messageGroups.length === 0 && (
            <p className="chat-app__status">Loading messages...</p>
          )}

          {!isLoading && messageGroups.length === 0 && (
            <p className="chat-app__status">No messages yet. Send something to start.</p>
          )}

          {groupsWithStreamEvents.map((group) => (
            <div key={group.id} className="chat-message-group">
              {/* User Message */}
              {group.userMessage && (
                <article
                  className={`chat-msg chat-msg--user ${group.userMessage.pending ? 'chat-msg--pending' : ''}`}
                >
                  <div className="chat-msg__meta">
                    <span>{group.userMessage.sender}</span>
                    <span>{new Date(group.userMessage.timestamp).toLocaleTimeString()}</span>
                  </div>
                  <p>{group.userMessage.text}</p>
                </article>
              )}

              {/* Assistant Message with Collapsible Sections */}
              {group.assistantMessage && (
                <article className="chat-msg">
                  <div className="chat-msg__meta">
                    <span>{group.assistantMessage.sender}</span>
                    <span>{new Date(group.assistantMessage.timestamp).toLocaleTimeString()}</span>
                  </div>

                  {/* Collapsible Reasoning Section */}
                  {group.reasoning.length > 0 && (
                    <CollapsibleSection
                      title={`Reasoning (${group.reasoning.length})`}
                      type="reasoning"
                      defaultExpanded={expandedGroups.has(group.id)}
                      onToggle={() => toggleGroupExpanded(group.id)}
                    >
                      {group.reasoning.map((event) => (
                        <div key={event.id} className="chat-collapsible__item">
                          <pre>{event.text}</pre>
                        </div>
                      ))}
                    </CollapsibleSection>
                  )}

                  {/* Collapsible Tool Calls Section */}
                  {group.toolCalls.length > 0 && (
                    <CollapsibleSection
                      title={`Tool Calls (${group.toolCalls.length})`}
                      type="tool_call"
                      defaultExpanded={expandedGroups.has(`${group.id}-tools`)}
                      onToggle={() => toggleGroupExpanded(`${group.id}-tools`)}
                    >
                      {group.toolCalls.map((event) => (
                        <div key={event.id} className="chat-collapsible__item">
                          <div className="chat-collapsible__meta">
                            {new Date(event.timestamp).toLocaleTimeString()}
                          </div>
                          <pre>{event.text}</pre>
                        </div>
                      ))}
                    </CollapsibleSection>
                  )}

                  {/* Collapsible Tool Results Section */}
                  {group.toolResults.length > 0 && (
                    <CollapsibleSection
                      title={`Tool Results (${group.toolResults.length})`}
                      type="tool_result"
                      defaultExpanded={expandedGroups.has(`${group.id}-results`)}
                      onToggle={() => toggleGroupExpanded(`${group.id}-results`)}
                    >
                      {group.toolResults.map((event) => (
                        <div key={event.id} className="chat-collapsible__item">
                          <div className="chat-collapsible__meta">
                            {new Date(event.timestamp).toLocaleTimeString()}
                          </div>
                          <pre>{event.text}</pre>
                        </div>
                      ))}
                    </CollapsibleSection>
                  )}

                  {/* Assistant Text */}
                  <p>{group.assistantMessage.text}</p>
                </article>
              )}

              {/* Live stream events when no assistant message yet */}
              {group.hasStreamEvents && !group.assistantMessage && (
                <section className="chat-stream" aria-live="polite">
                  <h4 className="chat-stream__title">Live activity</h4>
                  <div className="chat-stream__list">
                    {group.reasoning.map((event) => (
                      <article
                        key={event.id}
                        className="chat-stream__event chat-stream__event--thinking"
                      >
                        <div className="chat-stream__meta">
                          <span>reasoning</span>
                          <span>{new Date(event.timestamp).toLocaleTimeString()}</span>
                        </div>
                        <pre>{event.text}</pre>
                      </article>
                    ))}
                    {group.toolCalls.map((event) => (
                      <article
                        key={event.id}
                        className="chat-stream__event chat-stream__event--tool_call"
                      >
                        <div className="chat-stream__meta">
                          <span>tool call</span>
                          <span>{new Date(event.timestamp).toLocaleTimeString()}</span>
                        </div>
                        <pre>{event.text}</pre>
                      </article>
                    ))}
                    {group.toolResults.map((event) => (
                      <article
                        key={event.id}
                        className="chat-stream__event chat-stream__event--tool_result"
                      >
                        <div className="chat-stream__meta">
                          <span>tool result</span>
                          <span>{new Date(event.timestamp).toLocaleTimeString()}</span>
                        </div>
                        <pre>{event.text}</pre>
                      </article>
                    ))}
                  </div>
                </section>
              )}
            </div>
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
    </div>
  );
}

// Collapsible Section Component
interface CollapsibleSectionProps {
  title: string;
  type: 'reasoning' | 'tool_call' | 'tool_result';
  defaultExpanded?: boolean;
  onToggle?: () => void;
  children: React.ReactNode;
}

function CollapsibleSection({
  title,
  type,
  defaultExpanded = false,
  onToggle,
  children,
}: CollapsibleSectionProps) {
  const [isExpanded, setIsExpanded] = useState(defaultExpanded);

  const handleToggle = () => {
    const newState = !isExpanded;
    setIsExpanded(newState);
    onToggle?.();
  };

  return (
    <div className={`chat-collapsible chat-collapsible--${type}`}>
      <button
        type="button"
        className="chat-collapsible__header"
        onClick={handleToggle}
        aria-expanded={isExpanded}
      >
        <span className="chat-collapsible__icon">{isExpanded ? '▼' : '▶'}</span>
        <span className="chat-collapsible__title">{title}</span>
      </button>
      {isExpanded && <div className="chat-collapsible__content">{children}</div>}
    </div>
  );
}
