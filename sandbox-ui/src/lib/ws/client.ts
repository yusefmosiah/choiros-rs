import type { WsClientMessage, WsConnectionStatus, WsServerMessage } from './types';
import { parseWsServerMessage } from './types';

const DEFAULT_BACKOFF_MS = 500;
const MAX_BACKOFF_MS = 30_000;

export interface WebSocketClientOptions {
  url?: string;
  maxBackoffMs?: number;
  baseBackoffMs?: number;
}

type MessageListener = (message: WsServerMessage) => void;
type StatusListener = (status: WsConnectionStatus) => void;
type ErrorListener = (error: Error) => void;

export class DesktopWebSocketClient {
  private socket: WebSocket | null = null;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private reconnectAttempt = 0;
  private intentionalClose = false;
  private status: WsConnectionStatus = 'disconnected';
  private desktopId: string | null = null;

  private readonly wsUrl: string;
  private readonly maxBackoffMs: number;
  private readonly baseBackoffMs: number;
  private readonly messageListeners = new Set<MessageListener>();
  private readonly statusListeners = new Set<StatusListener>();
  private readonly errorListeners = new Set<ErrorListener>();

  constructor(options: WebSocketClientOptions = {}) {
    this.wsUrl = options.url ?? resolveWebSocketUrl();
    this.maxBackoffMs = options.maxBackoffMs ?? MAX_BACKOFF_MS;
    this.baseBackoffMs = options.baseBackoffMs ?? DEFAULT_BACKOFF_MS;
  }

  getStatus(): WsConnectionStatus {
    return this.status;
  }

  connect(desktopId: string): void {
    this.desktopId = desktopId;
    this.intentionalClose = false;
    this.clearReconnectTimer();

    if (this.socket && this.socket.readyState === WebSocket.OPEN) {
      this.send({ type: 'subscribe', desktop_id: desktopId });
      return;
    }

    if (this.socket && this.socket.readyState === WebSocket.CONNECTING) {
      return;
    }

    this.openSocket(this.reconnectAttempt > 0 ? 'reconnecting' : 'connecting');
  }

  disconnect(): void {
    this.intentionalClose = true;
    this.clearReconnectTimer();

    if (this.socket) {
      this.socket.close();
      this.socket = null;
    }

    this.setStatus('disconnected');
  }

  send(message: WsClientMessage): void {
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) {
      return;
    }

    this.socket.send(JSON.stringify(message));
  }

  ping(): void {
    this.send({ type: 'ping' });
  }

  onMessage(listener: MessageListener): () => void {
    this.messageListeners.add(listener);
    return () => {
      this.messageListeners.delete(listener);
    };
  }

  onStatusChange(listener: StatusListener): () => void {
    this.statusListeners.add(listener);
    listener(this.status);

    return () => {
      this.statusListeners.delete(listener);
    };
  }

  onError(listener: ErrorListener): () => void {
    this.errorListeners.add(listener);
    return () => {
      this.errorListeners.delete(listener);
    };
  }

  private openSocket(status: WsConnectionStatus): void {
    this.setStatus(status);

    try {
      this.socket = new WebSocket(this.wsUrl);
    } catch {
      this.scheduleReconnect();
      return;
    }

    this.socket.onopen = () => {
      this.reconnectAttempt = 0;
      this.setStatus('connected');

      if (this.desktopId) {
        this.send({ type: 'subscribe', desktop_id: this.desktopId });
      }
    };

    this.socket.onmessage = (event) => {
      const raw = typeof event.data === 'string' ? event.data : '';
      if (!raw) {
        return;
      }

      const message = parseWsServerMessage(raw);
      if (!message) {
        return;
      }

      this.messageListeners.forEach((listener) => {
        listener(message);
      });
    };

    this.socket.onerror = () => {
      this.errorListeners.forEach((listener) => {
        listener(new Error('WebSocket error'));
      });
    };

    this.socket.onclose = () => {
      this.socket = null;

      if (this.intentionalClose) {
        this.setStatus('disconnected');
        return;
      }

      this.scheduleReconnect();
    };
  }

  private scheduleReconnect(): void {
    if (!this.desktopId) {
      this.setStatus('disconnected');
      return;
    }

    this.clearReconnectTimer();
    this.setStatus('reconnecting');

    const delay = Math.min(this.maxBackoffMs, this.baseBackoffMs * 2 ** this.reconnectAttempt);
    this.reconnectAttempt += 1;

    this.reconnectTimer = setTimeout(() => {
      if (this.desktopId && !this.intentionalClose) {
        this.openSocket('reconnecting');
      }
    }, delay);
  }

  private clearReconnectTimer(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  private setStatus(status: WsConnectionStatus): void {
    this.status = status;
    this.statusListeners.forEach((listener) => {
      listener(status);
    });
  }
}

export function resolveWebSocketUrl(): string {
  const explicitWsUrl = import.meta.env.VITE_WS_URL;
  if (explicitWsUrl) {
    return explicitWsUrl;
  }

  const apiUrl = import.meta.env.VITE_API_URL;
  if (apiUrl) {
    return httpToWsUrl(apiUrl) + '/ws';
  }

  if (typeof window !== 'undefined') {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    return `${protocol}//${window.location.host}/ws`;
  }

  return 'ws://localhost:8080/ws';
}

export function httpToWsUrl(url: string): string {
  if (url.startsWith('https://')) {
    return url.replace('https://', 'wss://');
  }

  if (url.startsWith('http://')) {
    return url.replace('http://', 'ws://');
  }

  if (url.startsWith('ws://') || url.startsWith('wss://')) {
    return url;
  }

  return `ws://${url}`;
}
