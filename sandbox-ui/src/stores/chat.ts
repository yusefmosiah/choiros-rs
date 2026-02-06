import { create } from 'zustand';
import type { ChatMessage } from '@/types/generated';

interface ChatState {
  messages: ChatMessage[];
  isLoading: boolean;
  error: string | null;
}

interface ChatStore {
  chats: Map<string, ChatState>;
  getChatState: (actorId: string) => ChatState;
  setMessages: (actorId: string, messages: ChatMessage[]) => void;
  addMessage: (actorId: string, message: ChatMessage) => void;
  updatePendingMessage: (actorId: string, id: string, pending: boolean) => void;
  setLoading: (actorId: string, loading: boolean) => void;
  setError: (actorId: string, message: string | null) => void;
  clear: (actorId: string) => void;
  clearAll: () => void;
}

const getInitialState = (): ChatState => ({
  messages: [],
  isLoading: false,
  error: null,
});

export const useChatStore = create<ChatStore>((set, get) => ({
  chats: new Map(),

  getChatState: (actorId: string) => {
    const state = get();
    return state.chats.get(actorId) ?? getInitialState();
  },

  setMessages: (actorId: string, messages: ChatMessage[]) => {
    set((state) => {
      const newChats = new Map(state.chats);
      const chatState = newChats.get(actorId) ?? getInitialState();
      newChats.set(actorId, { ...chatState, messages });
      return { chats: newChats };
    });
  },

  addMessage: (actorId: string, message: ChatMessage) => {
    set((state) => {
      const newChats = new Map(state.chats);
      const chatState = newChats.get(actorId) ?? getInitialState();
      newChats.set(actorId, {
        ...chatState,
        messages: [...chatState.messages, message],
      });
      return { chats: newChats };
    });
  },

  updatePendingMessage: (actorId: string, id: string, pending: boolean) => {
    set((state) => {
      const newChats = new Map(state.chats);
      const chatState = newChats.get(actorId);
      if (!chatState) return state;

      newChats.set(actorId, {
        ...chatState,
        messages: chatState.messages.map((message) => {
          if (message.id !== id) {
            return message;
          }
          return { ...message, pending };
        }),
      });
      return { chats: newChats };
    });
  },

  setLoading: (actorId: string, loading: boolean) => {
    set((state) => {
      const newChats = new Map(state.chats);
      const chatState = newChats.get(actorId) ?? getInitialState();
      newChats.set(actorId, { ...chatState, isLoading: loading });
      return { chats: newChats };
    });
  },

  setError: (actorId: string, message: string | null) => {
    set((state) => {
      const newChats = new Map(state.chats);
      const chatState = newChats.get(actorId) ?? getInitialState();
      newChats.set(actorId, { ...chatState, error: message });
      return { chats: newChats };
    });
  },

  clear: (actorId: string) => {
    set((state) => {
      const newChats = new Map(state.chats);
      newChats.delete(actorId);
      return { chats: newChats };
    });
  },

  clearAll: () => {
    set({ chats: new Map() });
  },
}));

// Hook to get chat store selectors for a specific actor
export function useChatStoreForActor(actorId: string) {
  const store = useChatStore();
  const chatState = store.getChatState(actorId);

  return {
    messages: chatState.messages,
    isLoading: chatState.isLoading,
    error: chatState.error,
    setMessages: (messages: ChatMessage[]) => store.setMessages(actorId, messages),
    addMessage: (message: ChatMessage) => store.addMessage(actorId, message),
    updatePendingMessage: (id: string, pending: boolean) =>
      store.updatePendingMessage(actorId, id, pending),
    setLoading: (loading: boolean) => store.setLoading(actorId, loading),
    setError: (message: string | null) => store.setError(actorId, message),
    clear: () => store.clear(actorId),
  };
}
