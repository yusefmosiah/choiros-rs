import { useCallback, useEffect, useMemo, useState } from 'react';
import { getMessages, sendMessage } from '@/lib/api/chat';
import type { ChatMessage } from '@/types/generated';
import { useChatStore } from '@/stores/chat';
import './Chat.css';

interface ChatProps {
  actorId: string;
  userId?: string;
}

function sortMessages(messages: ChatMessage[]): ChatMessage[] {
  return [...messages].sort((a, b) => a.timestamp.localeCompare(b.timestamp));
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
    void loadMessages();

    const interval = setInterval(() => {
      void loadMessages();
    }, 2500);

    return () => {
      clearInterval(interval);
    };
  }, [loadMessages]);

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

    setDraft('');

    try {
      await sendMessage(actorId, {
        text,
        user_id: userId,
      });

      updatePendingMessage(tempId, false);
      setError(null);

      // Refresh after backend processing trigger.
      setTimeout(() => {
        void loadMessages();
      }, 400);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to send message');
    }
  }, [actorId, addMessage, draft, loadMessages, setError, updatePendingMessage, userId]);

  const renderedMessages = useMemo(() => sortMessages(messages), [messages]);

  return (
    <div className="chat-app">
      <header className="chat-app__header">
        <h3>Chat</h3>
        <button type="button" onClick={() => void loadMessages()} disabled={isLoading}>
          Refresh
        </button>
      </header>

      <div className="chat-app__messages">
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
