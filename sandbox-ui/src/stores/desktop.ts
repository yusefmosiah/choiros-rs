import { create } from 'zustand';
import type { AppDefinition, DesktopState } from '@/types/generated';

interface DesktopStore {
  desktop: DesktopState | null;
  activeWindowId: string | null;
  wsConnected: boolean;
  lastError: string | null;
  setDesktopState: (desktop: DesktopState) => void;
  setActiveWindow: (windowId: string | null) => void;
  setWsConnected: (connected: boolean) => void;
  setError: (message: string | null) => void;
  registerApp: (app: AppDefinition) => void;
  closeWindow: (windowId: string) => void;
  minimizeWindow: (windowId: string) => void;
  reset: () => void;
}

export const useDesktopStore = create<DesktopStore>((set) => ({
  desktop: null,
  activeWindowId: null,
  wsConnected: false,
  lastError: null,

  setDesktopState: (desktop) => {
    set({ desktop, activeWindowId: desktop.active_window, lastError: null });
  },

  setActiveWindow: (windowId) => {
    set((state) => {
      if (!state.desktop) {
        return { activeWindowId: windowId };
      }

      return {
        activeWindowId: windowId,
        desktop: {
          ...state.desktop,
          active_window: windowId,
        },
      };
    });
  },

  setWsConnected: (connected) => {
    set({ wsConnected: connected });
  },

  setError: (message) => {
    set({ lastError: message });
  },

  registerApp: (app) => {
    set((state) => {
      if (!state.desktop) {
        return state;
      }

      return {
        desktop: {
          ...state.desktop,
          apps: [...state.desktop.apps, app],
        },
      };
    });
  },

  closeWindow: (windowId) => {
    set((state) => {
      if (!state.desktop) {
        return state;
      }

      const nextActive =
        state.activeWindowId === windowId
          ? state.desktop.windows[state.desktop.windows.length - 1]?.id ?? null
          : state.activeWindowId;

      return {
        activeWindowId: nextActive,
        desktop: {
          ...state.desktop,
          active_window: nextActive,
        },
      };
    });
  },

  minimizeWindow: (windowId) => {
    set((state) => {
      if (!state.desktop || state.activeWindowId !== windowId) {
        return state;
      }

      const nextActive = state.desktop.windows
        .filter((window) => !window.minimized && window.id !== windowId)
        .reduce<{ id: string; z_index: number } | null>((current, window) => {
          if (!current || window.z_index > current.z_index) {
            return { id: window.id, z_index: window.z_index };
          }

          return current;
        }, null)?.id ?? null;

      return {
        activeWindowId: nextActive,
        desktop: {
          ...state.desktop,
          active_window: nextActive,
        },
      };
    });
  },

  reset: () => {
    set({
      desktop: null,
      activeWindowId: null,
      wsConnected: false,
      lastError: null,
    });
  },
}));
