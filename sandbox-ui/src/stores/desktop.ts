import { create } from 'zustand';
import type { AppDefinition } from '@/types/generated';

interface DesktopStore {
  // Desktop metadata (not windows array - that's in WindowsStore)
  apps: AppDefinition[];
  activeWindowId: string | null;
  wsConnected: boolean;
  lastError: string | null;
  // Actions
  setApps: (apps: AppDefinition[]) => void;
  setActiveWindow: (windowId: string | null) => void;
  setWsConnected: (connected: boolean) => void;
  setError: (message: string | null) => void;
  registerApp: (app: AppDefinition) => void;
  reset: () => void;
}

export const useDesktopStore = create<DesktopStore>((set) => ({
  apps: [],
  activeWindowId: null,
  wsConnected: false,
  lastError: null,

  // Set apps array - NOT windows array (that's in WindowsStore)
  setApps: (apps) => {
    set({ apps, lastError: null });
  },

  // Set active window ID only - windows array is managed by WindowsStore
  setActiveWindow: (windowId) => {
    set({ activeWindowId: windowId });
  },

  setWsConnected: (connected) => {
    set({ wsConnected: connected });
  },

  setError: (message) => {
    set({ lastError: message });
  },

  registerApp: (app) => {
    set((state) => ({
      apps: [...state.apps, app],
    }));
  },

  reset: () => {
    set({
      apps: [],
      activeWindowId: null,
      wsConnected: false,
      lastError: null,
    });
  },
}));
