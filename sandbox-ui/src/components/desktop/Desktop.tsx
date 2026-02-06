import { useCallback, useEffect, useMemo, useState } from 'react';
import type { AppDefinition } from '@/types/generated';
import { useWebSocket } from '@/hooks/useWebSocket';
import { CORE_APPS } from '@/lib/apps';

// Module-level bootstrap tracking to survive React StrictMode double-render
const bootstrapState = new Map<string, boolean>();
import {
  closeWindow,
  focusWindow,
  getApps,
  maximizeWindow,
  minimizeWindow,
  moveWindow,
  openWindow,
  registerApp,
  resizeWindow,
  restoreWindow,
} from '@/lib/api/desktop';
import { sendMessage } from '@/lib/api/chat';
import { useDesktopStore } from '@/stores/desktop';
import { useWindowsStore } from '@/stores/windows';
import { Icon } from './Icon';
import { PromptBar } from './PromptBar';
import { WindowManager } from '@/components/window/WindowManager';
import './Desktop.css';
import '@/components/window/Window.css';

interface DesktopProps {
  desktopId?: string;
}

export function Desktop({ desktopId = 'desktop-1' }: DesktopProps) {
  const { status } = useWebSocket(desktopId);
  const windows = useWindowsStore((state) => state.windows);

  const activeWindowId = useDesktopStore((state) => state.activeWindowId);
  const wsError = useDesktopStore((state) => state.lastError);
  const setDesktopError = useDesktopStore((state) => state.setError);

  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Bootstrap: Wait for WebSocket connection before fetching state
  const maxRetries = 3;
  const retryCountRef = { current: 0 };

  useEffect(() => {
    if (!loading) {
      return;
    }

    if (status === 'connected') {
      return;
    }

    const timeout = setTimeout(() => {
      const message = wsError ?? 'Desktop connection timed out';
      setError(message);
      setDesktopError(message);
      setLoading(false);
    }, 8000);

    return () => {
      clearTimeout(timeout);
    };
  }, [loading, setDesktopError, status, wsError]);

  useEffect(() => {
    let cancelled = false;
    const bootstrapKey = `${desktopId}-bootstrap`;

    const bootstrap = async () => {
      // Only run bootstrap once per connection (module-level tracking for StrictMode)
      if (bootstrapState.get(bootstrapKey) || status !== 'connected') {
        return;
      }
      bootstrapState.set(bootstrapKey, true);

      try {
        const existingApps = await getApps(desktopId).catch(() => []);
        const existingIds = new Set(existingApps.map((app) => app.id));
        const missingApps = CORE_APPS.filter((app) => !existingIds.has(app.id));

        if (missingApps.length > 0) {
          await Promise.all(
            missingApps.map(async (app) => {
              await registerApp(desktopId, app);
            }),
          );
        }

        // Note: Desktop state comes via WebSocket (desktop_state message)
        // We only need to register apps here, not fetch state via API
        setError(null);
        retryCountRef.current = 0; // Reset retry count on success
      } catch (err) {
        // Reset bootstrap state on error to allow retry
        bootstrapState.delete(bootstrapKey);

        if (cancelled) {
          return;
        }

        const message = err instanceof Error ? err.message : 'Failed to load desktop state';
        setError(message);
        setDesktopError(message);

        // Retry on failure (up to maxRetries)
        if (retryCountRef.current < maxRetries) {
          retryCountRef.current++;
          setTimeout(() => {
            if (!cancelled) {
              void bootstrap();
            }
          }, 1000 * retryCountRef.current); // Exponential backoff
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    };

    void bootstrap();

    return () => {
      cancelled = true;
    };
  }, [desktopId, setDesktopError, status]);

  const handleOpenApp = useCallback(
    async (app: AppDefinition) => {
      try {
        await openWindow(desktopId, {
          app_id: app.id,
          title: app.name,
        });
      } catch (err) {
        const message = err instanceof Error ? err.message : `Failed to open ${app.name}`;
        setError(message);
        setDesktopError(message);
      }
    },
    [desktopId, setDesktopError],
  );

  const handleCloseWindow = useCallback(
    async (windowId: string) => {
      try {
        // Check if window exists before attempting to close
        const window = useWindowsStore.getState().windows.find((w) => w.id === windowId);
        if (!window) {
          console.warn(`Window ${windowId} not found in local state, may already be closed`);
          return;
        }
        await closeWindow(desktopId, windowId);
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Failed to close window';
        setError(message);
        setDesktopError(message);
      }
    },
    [desktopId, setDesktopError],
  );

  const handleFocusWindow = useCallback(
    async (windowId: string) => {
      try {
        const window = useWindowsStore.getState().windows.find((w) => w.id === windowId);

        // If window is minimized, restore it first before focusing
        // Backend rejects focus operations on minimized windows
        if (window?.minimized) {
          await restoreWindow(desktopId, windowId);
          return;
        }

        await focusWindow(desktopId, windowId);
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Failed to focus window';
        setError(message);
        setDesktopError(message);
      }
    },
    [desktopId, setDesktopError],
  );

  const handleActivateWindow = useCallback(
    async (windowId: string) => {
      try {
        const window = useWindowsStore.getState().windows.find((item) => item.id === windowId);
        if (window?.minimized) {
          await restoreWindow(desktopId, windowId);
          return;
        }

        await focusWindow(desktopId, windowId);
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Failed to activate window';
        setError(message);
        setDesktopError(message);
      }
    },
    [desktopId, setDesktopError],
  );

  const handleMoveWindow = useCallback(
    async (windowId: string, x: number, y: number) => {
      try {
        // Validate window exists before operation
        const window = useWindowsStore.getState().windows.find((w) => w.id === windowId);
        if (!window) {
          console.warn(`Window ${windowId} not found, skipping move`);
          return;
        }
        await moveWindow(desktopId, windowId, x, y);
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Failed to move window';
        setError(message);
        setDesktopError(message);
      }
    },
    [desktopId, setDesktopError],
  );

  const handleResizeWindow = useCallback(
    async (windowId: string, width: number, height: number) => {
      try {
        // Validate window exists before operation
        const window = useWindowsStore.getState().windows.find((w) => w.id === windowId);
        if (!window) {
          console.warn(`Window ${windowId} not found, skipping resize`);
          return;
        }
        await resizeWindow(desktopId, windowId, width, height);
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Failed to resize window';
        setError(message);
        setDesktopError(message);
      }
    },
    [desktopId, setDesktopError],
  );

  const handleMinimizeWindow = useCallback(
    async (windowId: string) => {
      try {
        // Validate window exists before operation
        const window = useWindowsStore.getState().windows.find((w) => w.id === windowId);
        if (!window) {
          console.warn(`Window ${windowId} not found, skipping minimize`);
          return;
        }
        await minimizeWindow(desktopId, windowId);
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Failed to minimize window';
        setError(message);
        setDesktopError(message);
      }
    },
    [desktopId, setDesktopError],
  );

  const handleMaximizeWindow = useCallback(
    async (windowId: string) => {
      try {
        // Validate window exists before operation
        const window = useWindowsStore.getState().windows.find((w) => w.id === windowId);
        if (!window) {
          console.warn(`Window ${windowId} not found, skipping maximize`);
          return;
        }
        const response = await maximizeWindow(desktopId, windowId);
        // Optimistically update local state in case WebSocket is lagging
        useWindowsStore.getState().maximizeWindow(
          windowId,
          response.window.x,
          response.window.y,
          response.window.width,
          response.window.height,
        );
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Failed to maximize window';
        setError(message);
        setDesktopError(message);
      }
    },
    [desktopId, setDesktopError],
  );

  const handleRestoreWindow = useCallback(
    async (windowId: string) => {
      try {
        // Validate window exists before operation
        const window = useWindowsStore.getState().windows.find((w) => w.id === windowId);
        if (!window) {
          console.warn(`Window ${windowId} not found, skipping restore`);
          return;
        }
        const response = await restoreWindow(desktopId, windowId);
        // Optimistically update local state in case WebSocket is lagging
        useWindowsStore.getState().restoreWindow(
          windowId,
          response.window.x,
          response.window.y,
          response.window.width,
          response.window.height,
        );
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Failed to restore window';
        setError(message);
        setDesktopError(message);
      }
    },
    [desktopId, setDesktopError],
  );

  const handlePromptSubmit = useCallback(
    async (text: string) => {
      try {
        const chatWindow = useWindowsStore.getState().windows.find((window) => window.app_id === 'chat');

        if (chatWindow) {
          await handleActivateWindow(chatWindow.id);
          await sendMessage(chatWindow.id, { text, user_id: 'user-1' });
          return;
        }

        const opened = await openWindow(desktopId, {
          app_id: 'chat',
          title: 'Chat',
        });

        await sendMessage(opened.id, { text, user_id: 'user-1' });
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Failed to submit prompt';
        setError(message);
        setDesktopError(message);
      }
    },
    [desktopId, handleActivateWindow, setDesktopError],
  );

  const sortedWindows = useMemo(
    () => [...windows].sort((a, b) => a.z_index - b.z_index),
    [windows],
  );

  return (
    <main className="desktop-shell">
      <section className="desktop-workspace">
        <div className="desktop-icons">
          {CORE_APPS.map((app) => (
            <Icon key={app.id} app={app} onOpen={handleOpenApp} />
          ))}
        </div>

        {loading && <div className="desktop-state desktop-state--loading">Loading desktop...</div>}
        {!loading && error && <div className="desktop-state desktop-state--error">{error}</div>}

        {!loading && !error && (
          <WindowManager
            windows={sortedWindows}
            activeWindowId={activeWindowId}
            onClose={handleCloseWindow}
            onFocus={handleFocusWindow}
            onMove={handleMoveWindow}
            onResize={handleResizeWindow}
            onMinimize={handleMinimizeWindow}
            onMaximize={handleMaximizeWindow}
            onRestore={handleRestoreWindow}
          />
        )}
      </section>

      <PromptBar
        connected={status === 'connected'}
        windows={sortedWindows}
        activeWindowId={activeWindowId}
        onSubmit={handlePromptSubmit}
        onFocusWindow={handleActivateWindow}
      />
    </main>
  );
}
