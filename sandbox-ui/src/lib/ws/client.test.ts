import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { DesktopWebSocketClient, resolveWebSocketUrl, httpToWsUrl } from './client';
import type { WsServerMessage } from './types';

// Mock WebSocket
(globalThis as unknown as { WebSocket: typeof WebSocket }).WebSocket = vi.fn() as unknown as typeof WebSocket;

class MockWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;

  readyState = MockWebSocket.CONNECTING;
  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onerror: (() => void) | null = null;

  constructor(public url: string) {
    // Simulate connection opening immediately
    setTimeout(() => {
      this.readyState = MockWebSocket.OPEN;
      this.onopen?.();
    }, 0);
  }

  send(_data: string) {
    if (this.readyState !== MockWebSocket.OPEN) {
      throw new Error('WebSocket is not open');
    }
  }

  close() {
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.();
  }
}

(globalThis as unknown as { WebSocket: typeof WebSocket }).WebSocket = MockWebSocket as unknown as typeof WebSocket;

describe('DesktopWebSocketClient', () => {
  let client: DesktopWebSocketClient;

  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    client = new DesktopWebSocketClient({ url: 'ws://localhost:8080/ws' });
  });

  afterEach(() => {
    client.disconnect();
    vi.useRealTimers();
  });

  describe('connection lifecycle', () => {
    it('starts with disconnected status', () => {
      expect(client.getStatus()).toBe('disconnected');
    });

    it('transitions to connecting then connected', async () => {
      const statusChanges: string[] = [];
      client.onStatusChange((status) => statusChanges.push(status));

      client.connect('desktop-1');
      expect(client.getStatus()).toBe('connecting');

      // Wait for mock WebSocket to open
      await vi.advanceTimersByTimeAsync(10);

      expect(client.getStatus()).toBe('connected');
      expect(statusChanges).toContain('connecting');
      expect(statusChanges).toContain('connected');
    });

    it('sends subscribe message after connection', async () => {
      const sendSpy = vi.spyOn(MockWebSocket.prototype, 'send');

      client.connect('desktop-1');
      await vi.advanceTimersByTimeAsync(10);

      expect(sendSpy).toHaveBeenCalledWith(JSON.stringify({ type: 'subscribe', desktop_id: 'desktop-1' }));
    });

    it('allows disconnection', async () => {
      client.connect('desktop-1');
      await vi.advanceTimersByTimeAsync(10);
      expect(client.getStatus()).toBe('connected');

      client.disconnect();
      expect(client.getStatus()).toBe('disconnected');
    });

    it('does not reconnect on intentional disconnect', async () => {
      client.connect('desktop-1');
      await vi.advanceTimersByTimeAsync(10);

      client.disconnect();

      // Fast-forward past any potential reconnect delays
      await vi.advanceTimersByTimeAsync(60000);
      expect(client.getStatus()).toBe('disconnected');
    });
  });

  describe('reconnection', () => {
    it('schedules reconnect with exponential backoff on close', async () => {
      client.connect('desktop-1');
      await vi.advanceTimersByTimeAsync(10);

      // Simulate unexpected close
      const ws = (client as unknown as { socket: MockWebSocket }).socket;
      ws?.close();

      expect(client.getStatus()).toBe('reconnecting');

      // First retry at 500ms
      await vi.advanceTimersByTimeAsync(500);
      expect(client.getStatus()).toBe('reconnecting');

      // Wait for connection attempt
      await vi.advanceTimersByTimeAsync(10);
      expect(client.getStatus()).toBe('connected');
    });

    it('increases backoff on multiple failures', async () => {
      const maxBackoff = 5000;
      client = new DesktopWebSocketClient({
        url: 'ws://localhost:8080/ws',
        baseBackoffMs: 500,
        maxBackoffMs: maxBackoff,
      });

      client.connect('desktop-1');

      // First attempt: 0ms (immediate)
      await vi.advanceTimersByTimeAsync(10);

      // Simulate multiple disconnects
      for (let i = 0; i < 5; i++) {
        const ws = (client as unknown as { socket: MockWebSocket }).socket;
        ws?.close();

        // Each retry doubles: 500, 1000, 2000, 4000, 5000 (capped)
        const expectedDelay = Math.min(maxBackoff, 500 * 2 ** i);
        await vi.advanceTimersByTimeAsync(expectedDelay + 10);
      }

      expect(client.getStatus()).toBe('connected');
    });
  });

  describe('message handling', () => {
    it('receives and parses messages', async () => {
      const messages: WsServerMessage[] = [];
      client.onMessage((msg) => messages.push(msg));

      client.connect('desktop-1');
      await vi.advanceTimersByTimeAsync(10);

      const ws = (client as unknown as { socket: MockWebSocket }).socket;
      const pongMessage: WsServerMessage = { type: 'pong' };
      ws?.onmessage?.({ data: JSON.stringify(pongMessage) });

      expect(messages).toHaveLength(1);
      expect(messages[0]).toEqual({ type: 'pong' });
    });

    it('handles desktop_state messages', async () => {
      const messages: WsServerMessage[] = [];
      client.onMessage((msg) => messages.push(msg));

      client.connect('desktop-1');
      await vi.advanceTimersByTimeAsync(10);

      const ws = (client as unknown as { socket: MockWebSocket }).socket;
      const stateMessage: WsServerMessage = {
        type: 'desktop_state',
        desktop: {
          windows: [],
          active_window: null,
          apps: [],
        },
      };
      ws?.onmessage?.({ data: JSON.stringify(stateMessage) });

      expect(messages[0].type).toBe('desktop_state');
    });

    it('ignores invalid messages', async () => {
      const messages: WsServerMessage[] = [];
      client.onMessage((msg) => messages.push(msg));

      client.connect('desktop-1');
      await vi.advanceTimersByTimeAsync(10);

      const ws = (client as unknown as { socket: MockWebSocket }).socket;
      ws?.onmessage?.({ data: 'invalid-json' });
      ws?.onmessage?.({ data: '{"unknown":"field"}' });

      expect(messages).toHaveLength(0);
    });

    it('ignores empty messages', async () => {
      const messages: WsServerMessage[] = [];
      client.onMessage((msg) => messages.push(msg));

      client.connect('desktop-1');
      await vi.advanceTimersByTimeAsync(10);

      const ws = (client as unknown as { socket: MockWebSocket }).socket;
      ws?.onmessage?.({ data: '' });

      expect(messages).toHaveLength(0);
    });
  });

  describe('ping', () => {
    it('sends ping message', async () => {
      const sendSpy = vi.spyOn(MockWebSocket.prototype, 'send');

      client.connect('desktop-1');
      await vi.advanceTimersByTimeAsync(10);

      client.ping();

      expect(sendSpy).toHaveBeenCalledWith(JSON.stringify({ type: 'ping' }));
    });

    it('does not send ping when disconnected', () => {
      const sendSpy = vi.spyOn(MockWebSocket.prototype, 'send');

      client.ping();

      expect(sendSpy).not.toHaveBeenCalled();
    });
  });

  describe('error handling', () => {
    it('emits errors on WebSocket error', async () => {
      const errors: Error[] = [];
      client.onError((err) => errors.push(err));

      client.connect('desktop-1');
      await vi.advanceTimersByTimeAsync(10);

      const ws = (client as unknown as { socket: MockWebSocket }).socket;
      ws?.onerror?.();

      expect(errors).toHaveLength(1);
      expect(errors[0].message).toBe('WebSocket error');
    });
  });

  describe('subscription management', () => {
    it('unsubscribes message listeners', async () => {
      const messages: WsServerMessage[] = [];
      const unsubscribe = client.onMessage((msg) => messages.push(msg));

      client.connect('desktop-1');
      await vi.advanceTimersByTimeAsync(10);

      unsubscribe();

      const ws = (client as unknown as { socket: MockWebSocket }).socket;
      ws?.onmessage?.({ data: '{"type":"pong"}' });

      expect(messages).toHaveLength(0);
    });

    it('unsubscribes status listeners', async () => {
      const statuses: string[] = [];
      const unsubscribe = client.onStatusChange((status) => statuses.push(status));

      unsubscribe();

      client.connect('desktop-1');
      await vi.advanceTimersByTimeAsync(10);

      expect(statuses.filter(s => s === 'connected')).toHaveLength(0);
    });
  });
});

describe('resolveWebSocketUrl', () => {
  // Note: Testing resolveWebSocketUrl with import.meta.env is challenging in Vitest
  // The core logic is covered by httpToWsUrl tests, and integration tests cover the full flow
  it('returns default localhost URL when no env vars set', () => {
    // This will use the default fallback
    expect(resolveWebSocketUrl()).toBe('ws://localhost:8080/ws');
  });
});

describe('httpToWsUrl', () => {
  it('converts http to ws', () => {
    expect(httpToWsUrl('http://localhost:8080')).toBe('ws://localhost:8080');
  });

  it('converts https to wss', () => {
    expect(httpToWsUrl('https://api.example.com')).toBe('wss://api.example.com');
  });

  it('preserves ws://', () => {
    expect(httpToWsUrl('ws://localhost:8080')).toBe('ws://localhost:8080');
  });

  it('preserves wss://', () => {
    expect(httpToWsUrl('wss://secure.example.com')).toBe('wss://secure.example.com');
  });

  it('adds ws:// prefix to plain host', () => {
    expect(httpToWsUrl('localhost:8080')).toBe('ws://localhost:8080');
  });
});
