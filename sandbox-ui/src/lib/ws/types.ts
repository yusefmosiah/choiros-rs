import type { AppDefinition, DesktopState, WindowState } from '@/types/generated';

export type WsConnectionStatus = 'disconnected' | 'connecting' | 'connected' | 'reconnecting';

export type WsClientMessage =
  | { type: 'subscribe'; desktop_id: string }
  | { type: 'ping' };

export type WsServerMessage =
  | { type: 'pong' }
  | { type: 'desktop_state'; desktop: DesktopState }
  | { type: 'window_opened'; window: WindowState }
  | { type: 'window_closed'; window_id: string }
  | { type: 'window_moved'; window_id: string; x: number; y: number }
  | { type: 'window_resized'; window_id: string; width: number; height: number }
  | { type: 'window_focused'; window_id: string; z_index: number }
  | { type: 'window_minimized'; window_id: string }
  | { type: 'window_maximized'; window_id: string; x: number; y: number; width: number; height: number }
  | {
      type: 'window_restored';
      window_id: string;
      x: number;
      y: number;
      width: number;
      height: number;
      from: string;
    }
  | { type: 'app_registered'; app: AppDefinition }
  | { type: 'error'; message: string };

export function parseWsServerMessage(raw: string): WsServerMessage | null {
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object') {
      return null;
    }

    const msg = parsed as { type?: unknown };
    if (typeof msg.type !== 'string') {
      return null;
    }

    return parsed as WsServerMessage;
  } catch {
    return null;
  }
}
