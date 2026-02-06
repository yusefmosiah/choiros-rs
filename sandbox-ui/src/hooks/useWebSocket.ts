import { useEffect, useMemo, useState } from 'react';
import { DesktopWebSocketClient } from '@/lib/ws/client';
import type { WsConnectionStatus, WsServerMessage } from '@/lib/ws/types';
import { useDesktopStore } from '@/stores/desktop';
import { useWindowsStore } from '@/stores/windows';

function applyWsMessage(message: WsServerMessage): void {
  const desktopStore = useDesktopStore.getState();
  const windowsStore = useWindowsStore.getState();

  switch (message.type) {
    case 'pong': {
      return;
    }
    case 'desktop_state': {
      desktopStore.setDesktopState(message.desktop);
      windowsStore.setWindows(message.desktop.windows);
      return;
    }
    case 'window_opened': {
      windowsStore.openWindow(message.window);
      desktopStore.setActiveWindow(message.window.id);
      return;
    }
    case 'window_closed': {
      windowsStore.closeWindow(message.window_id);
      desktopStore.closeWindow(message.window_id);
      return;
    }
    case 'window_moved': {
      windowsStore.moveWindow(message.window_id, message.x, message.y);
      return;
    }
    case 'window_resized': {
      windowsStore.resizeWindow(message.window_id, message.width, message.height);
      return;
    }
    case 'window_focused': {
      windowsStore.focusWindow(message.window_id, message.z_index);
      desktopStore.setActiveWindow(message.window_id);
      return;
    }
    case 'window_minimized': {
      windowsStore.minimizeWindow(message.window_id);
      desktopStore.minimizeWindow(message.window_id);
      return;
    }
    case 'window_maximized': {
      windowsStore.maximizeWindow(
        message.window_id,
        message.x,
        message.y,
        message.width,
        message.height,
      );
      desktopStore.setActiveWindow(message.window_id);
      return;
    }
    case 'window_restored': {
      windowsStore.restoreWindow(
        message.window_id,
        message.x,
        message.y,
        message.width,
        message.height,
      );
      desktopStore.setActiveWindow(message.window_id);
      return;
    }
    case 'app_registered': {
      desktopStore.registerApp(message.app);
      return;
    }
    case 'error': {
      desktopStore.setError(message.message);
      return;
    }
    default: {
      return;
    }
  }
}

function applyConnectionStatus(status: WsConnectionStatus): void {
  const desktopStore = useDesktopStore.getState();
  desktopStore.setWsConnected(status === 'connected');
}

export interface UseWebSocketResult {
  status: WsConnectionStatus;
  sendPing: () => void;
  disconnect: () => void;
}

export function useWebSocket(desktopId: string | null): UseWebSocketResult {
  const wsConnected = useDesktopStore((state) => state.wsConnected);
  const [status, setStatus] = useState<WsConnectionStatus>('disconnected');

  const client = useMemo(() => new DesktopWebSocketClient(), []);

  useEffect(() => {
    const unsubscribeMessage = client.onMessage((message) => {
      applyWsMessage(message);
    });

    const unsubscribeStatus = client.onStatusChange((status) => {
      applyConnectionStatus(status);
      setStatus(status);
    });

    const unsubscribeError = client.onError((error) => {
      useDesktopStore.getState().setError(error.message);
    });

    return () => {
      unsubscribeMessage();
      unsubscribeStatus();
      unsubscribeError();
    };
  }, [client]);

  useEffect(() => {
    if (!desktopId) {
      client.disconnect();
      return;
    }

    client.connect(desktopId);

    return () => {
      client.disconnect();
    };
  }, [client, desktopId]);

  return {
    status: wsConnected ? 'connected' : status,
    sendPing: () => {
      client.ping();
    },
    disconnect: () => {
      client.disconnect();
    },
  };
}
