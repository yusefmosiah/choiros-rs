// @vitest-environment jsdom
import { cleanup, render } from '@testing-library/react';
import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Terminal } from './Terminal';

const terminalApiMocks = vi.hoisted(() => ({
  createTerminalMock: vi.fn(async () => {}),
  stopTerminalMock: vi.fn(async () => {}),
  getTerminalWebSocketUrlMock: vi.fn(() => 'ws://localhost:8080/ws/terminal/test/user-1'),
}));

vi.mock('@/lib/api/terminal', () => ({
  createTerminal: terminalApiMocks.createTerminalMock,
  stopTerminal: terminalApiMocks.stopTerminalMock,
  getTerminalWebSocketUrl: terminalApiMocks.getTerminalWebSocketUrlMock,
}));

const xtermMocks = vi.hoisted(() => {
  class MockXTerm {
    static instances: MockXTerm[] = [];
    rows = 24;
    cols = 80;
    write = vi.fn<(data: string) => void>();
    open = vi.fn<(container: HTMLElement) => void>();
    loadAddon = vi.fn();
    dispose = vi.fn();
    onData = vi.fn(() => ({ dispose: vi.fn() }));

    constructor() {
      MockXTerm.instances.push(this);
    }
  }

  class MockFitAddon {
    static instances: MockFitAddon[] = [];
    fit = vi.fn();

    constructor() {
      MockFitAddon.instances.push(this);
    }
  }

  return { MockXTerm, MockFitAddon };
});

vi.mock('xterm', () => ({
  Terminal: xtermMocks.MockXTerm,
}));

vi.mock('xterm-addon-fit', () => ({
  FitAddon: xtermMocks.MockFitAddon,
}));

class MockResizeObserver {
  static instances: MockResizeObserver[] = [];
  callback: ResizeObserverCallback;
  observe = vi.fn();
  disconnect = vi.fn();
  private width = 640;
  private height = 360;

  constructor(callback: ResizeObserverCallback) {
    this.callback = callback;
    MockResizeObserver.instances.push(this);
  }

  trigger(width: number = this.width, height: number = this.height) {
    this.width = width;
    this.height = height;
    this.callback(
      [{ contentRect: { width, height } } as ResizeObserverEntry],
      this as unknown as ResizeObserver,
    );
  }
}

class MockWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;
  static instances: MockWebSocket[] = [];

  readyState = MockWebSocket.CONNECTING;
  onopen: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;
  onclose: ((event: Event) => void) | null = null;
  send = vi.fn<(data: string) => void>();
  close = vi.fn(() => {
    this.readyState = MockWebSocket.CLOSED;
  });

  constructor(public url: string) {
    MockWebSocket.instances.push(this);
  }

  emitClose() {
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.(new Event('close'));
  }
}

describe('Terminal component websocket lifecycle', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.stubGlobal('requestAnimationFrame', ((cb: FrameRequestCallback) =>
      setTimeout(() => cb(0), 0)) as typeof requestAnimationFrame);
    vi.stubGlobal('cancelAnimationFrame', ((id: number) => clearTimeout(id)) as typeof cancelAnimationFrame);
    vi.stubGlobal('ResizeObserver', MockResizeObserver);
    vi.stubGlobal('WebSocket', MockWebSocket);
    terminalApiMocks.createTerminalMock.mockClear();
    terminalApiMocks.stopTerminalMock.mockClear();
    terminalApiMocks.getTerminalWebSocketUrlMock.mockClear();
    MockWebSocket.instances = [];
    MockResizeObserver.instances = [];
    xtermMocks.MockXTerm.instances = [];
    xtermMocks.MockFitAddon.instances = [];
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it('reconnects after websocket close', async () => {
    render(<Terminal terminalId="term-1" userId="user-1" />);
    await act(async () => {
      await Promise.resolve();
    });

    expect(MockWebSocket.instances).toHaveLength(1);
    const firstWs = MockWebSocket.instances[0];

    await act(async () => {
      firstWs.emitClose();
      vi.advanceTimersByTime(500);
      await Promise.resolve();
    });

    expect(MockWebSocket.instances.length).toBeGreaterThan(1);
    expect(terminalApiMocks.createTerminalMock).toHaveBeenCalledTimes(2);
  });

  it('does not stop terminal on unmount', async () => {
    const view = render(<Terminal terminalId="term-2" userId="user-1" />);
    await act(async () => {
      await Promise.resolve();
    });

    await act(async () => {
      view.unmount();
      await Promise.resolve();
    });

    expect(terminalApiMocks.stopTerminalMock).not.toHaveBeenCalled();
  });

  it('does not spam resize messages when size is unchanged', async () => {
    render(<Terminal terminalId="term-3" userId="user-1" />);
    await act(async () => {
      await Promise.resolve();
    });

    expect(MockWebSocket.instances).toHaveLength(1);
    const ws = MockWebSocket.instances[0];
    await act(async () => {
      ws.readyState = MockWebSocket.OPEN;
      ws.onopen?.(new Event('open'));
      vi.runOnlyPendingTimers();
      await Promise.resolve();
    });

    expect(MockResizeObserver.instances).toHaveLength(1);
    const observer = MockResizeObserver.instances[0];

    await act(async () => {
      observer.trigger(640, 360);
      observer.trigger(640, 360);
      observer.trigger(640, 360);
      vi.runOnlyPendingTimers();
      await Promise.resolve();
    });

    const resizeMessages = ws.send.mock.calls
      .map(([raw]) => JSON.parse(String(raw)) as { type?: string })
      .filter((message) => message.type === 'resize');

    expect(resizeMessages).toHaveLength(1);
    expect(xtermMocks.MockFitAddon.instances[0].fit).toHaveBeenCalled();
  });

  it('sends a new resize message when terminal dimensions change', async () => {
    render(<Terminal terminalId="term-4" userId="user-1" />);
    await act(async () => {
      await Promise.resolve();
    });

    const ws = MockWebSocket.instances[0];
    await act(async () => {
      ws.readyState = MockWebSocket.OPEN;
      ws.onopen?.(new Event('open'));
      vi.runOnlyPendingTimers();
      await Promise.resolve();
    });

    const term = xtermMocks.MockXTerm.instances[0];
    const observer = MockResizeObserver.instances[0];

    await act(async () => {
      vi.runOnlyPendingTimers();
      await Promise.resolve();
    });

    term.rows = 40;
    term.cols = 120;

    await act(async () => {
      observer.trigger(700, 400);
      vi.runOnlyPendingTimers();
      await Promise.resolve();
    });

    const resizePayloads = ws.send.mock.calls
      .map(([raw]) => JSON.parse(String(raw)) as { type?: string; rows?: number; cols?: number })
      .filter((message) => message.type === 'resize');

    expect(resizePayloads[resizePayloads.length - 1]).toMatchObject({ rows: 40, cols: 120 });
  });
});
