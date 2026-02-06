import { useCallback, useEffect, useMemo, useState } from 'react';
import type { AppDefinition } from '@/types/generated';
import { useWebSocket } from '@/hooks/useWebSocket';
import { CORE_APPS } from '@/lib/apps';
import {
  closeWindow,
  focusWindow,
  getApps,
  getDesktopState,
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
import { Taskbar } from './Taskbar';
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
  const setDesktopState = useDesktopStore((state) => state.setDesktopState);
  const setDesktopError = useDesktopStore((state) => state.setError);

  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    const bootstrap = async () => {
      try {
        const existingApps = await getApps(desktopId).catch(() => []);
        const existingIds = new Set(existingApps.map((app) => app.id));
        const missingApps = CORE_APPS.filter((app) => !existingIds.has(app.id));

        await Promise.all(
          missingApps.map(async (app) => {
            await registerApp(desktopId, app);
          }),
        );

        const desktopState = await getDesktopState(desktopId);
        if (cancelled) {
          return;
        }

        setDesktopState(desktopState);
        useWindowsStore.getState().setWindows(desktopState.windows);
        setError(null);
      } catch (err) {
        if (cancelled) {
          return;
        }

        const message = err instanceof Error ? err.message : 'Failed to load desktop state';
        setError(message);
        setDesktopError(message);
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
  }, [desktopId, setDesktopError, setDesktopState]);

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
        await focusWindow(desktopId, windowId);
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Failed to focus window';
        setError(message);
        setDesktopError(message);
      }
    },
    [desktopId, setDesktopError],
  );

  const handleMoveWindow = useCallback(
    async (windowId: string, x: number, y: number) => {
      try {
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
        await maximizeWindow(desktopId, windowId);
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
        await restoreWindow(desktopId, windowId);
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
          await focusWindow(desktopId, chatWindow.id);
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
    [desktopId, setDesktopError],
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

      <Taskbar windows={sortedWindows} activeWindowId={activeWindowId} onFocusWindow={handleFocusWindow} />

      <PromptBar
        connected={status === 'connected'}
        windows={sortedWindows}
        activeWindowId={activeWindowId}
        onSubmit={handlePromptSubmit}
        onFocusWindow={handleFocusWindow}
      />
    </main>
  );
}
