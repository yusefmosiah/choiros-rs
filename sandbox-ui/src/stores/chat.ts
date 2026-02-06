import { create } from 'zustand';
import type { ChatMessage } from '@/types/generated';

interface ChatStore {
  messages: ChatMessage[];
  isLoading: boolean;
  error: string | null;
  setMessages: (messages: ChatMessage[]) => void;
  addMessage: (message: ChatMessage) => void;
  updatePendingMessage: (id: string, pending: boolean) => void;
  setLoading: (loading: boolean) => void;
  setError: (message: string | null) => void;
  clear: () => void;
}

export const useChatStore = create<ChatStore>((set) => ({
  messages: [],
  isLoading: false,
  error: null,

  setMessages: (messages) => {
    set({ messages });
  },

  addMessage: (message) => {
    set((state) => ({ messages: [...state.messages, message] }));
  },

  updatePendingMessage: (id, pending) => {
    set((state) => ({
      messages: state.messages.map((message) => {
        if (message.id !== id) {
          return message;
        }

        return {
          ...message,
          pending,
        };
      }),
    }));
  },

  setLoading: (loading) => {
    set({ isLoading: loading });
  },

  setError: (message) => {
    set({ error: message });
  },

  clear: () => {
    set({ messages: [], isLoading: false, error: null });
  },
}));
