import { create } from 'zustand';
import type { WindowState } from '@/types/generated';

interface WindowsStore {
  windows: WindowState[];
  setWindows: (windows: WindowState[]) => void;
  openWindow: (window: WindowState) => void;
  closeWindow: (windowId: string) => void;
  moveWindow: (windowId: string, x: number, y: number) => void;
  resizeWindow: (windowId: string, width: number, height: number) => void;
  focusWindow: (windowId: string, zIndex: number) => void;
  minimizeWindow: (windowId: string) => void;
  maximizeWindow: (windowId: string, x: number, y: number, width: number, height: number) => void;
  restoreWindow: (
    windowId: string,
    x: number,
    y: number,
    width: number,
    height: number,
  ) => void;
  reset: () => void;
}

function updateWindow(
  windows: WindowState[],
  windowId: string,
  updater: (window: WindowState) => WindowState,
): WindowState[] {
  return windows.map((window) => {
    if (window.id !== windowId) {
      return window;
    }

    return updater(window);
  });
}

export const useWindowsStore = create<WindowsStore>((set) => ({
  windows: [],
  setWindows: (windows) => {
    set({ windows });
  },
  openWindow: (window) => {
    set((state) => ({ windows: [...state.windows, window] }));
  },
  closeWindow: (windowId) => {
    set((state) => ({ windows: state.windows.filter((window) => window.id !== windowId) }));
  },
  moveWindow: (windowId, x, y) => {
    set((state) => ({
      windows: updateWindow(state.windows, windowId, (window) => ({ ...window, x, y })),
    }));
  },
  resizeWindow: (windowId, width, height) => {
    set((state) => ({
      windows: updateWindow(state.windows, windowId, (window) => ({ ...window, width, height })),
    }));
  },
  focusWindow: (windowId, zIndex) => {
    set((state) => ({
      windows: updateWindow(state.windows, windowId, (window) => ({
        ...window,
        z_index: zIndex,
        minimized: false,
      })),
    }));
  },
  minimizeWindow: (windowId) => {
    set((state) => ({
      windows: updateWindow(state.windows, windowId, (window) => ({
        ...window,
        minimized: true,
        maximized: false,
      })),
    }));
  },
  maximizeWindow: (windowId, x, y, width, height) => {
    set((state) => {
      const nextZ = state.windows.reduce((max, window) => Math.max(max, window.z_index), 0) + 1;
      return {
        windows: updateWindow(state.windows, windowId, (window) => ({
          ...window,
          minimized: false,
          maximized: true,
          x,
          y,
          width,
          height,
          z_index: nextZ,
        })),
      };
    });
  },
  restoreWindow: (windowId, x, y, width, height) => {
    set((state) => ({
      windows: updateWindow(state.windows, windowId, (window) => ({
        ...window,
        minimized: false,
        maximized: false,
        x,
        y,
        width,
        height,
      })),
    }));
  },
  reset: () => {
    set({ windows: [] });
  },
}));
