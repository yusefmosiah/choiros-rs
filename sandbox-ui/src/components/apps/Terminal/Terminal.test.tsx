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
    rows = 24;
    cols = 80;
    write = vi.fn<(data: string) => void>();
    open = vi.fn<(container: HTMLElement) => void>();
    loadAddon = vi.fn();
    dispose = vi.fn();
    onData = vi.fn(() => ({ dispose: vi.fn() }));
  }

  class MockFitAddon {
    fit = vi.fn();
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
  observe = vi.fn();
  disconnect = vi.fn();
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
    vi.stubGlobal('ResizeObserver', MockResizeObserver);
    vi.stubGlobal('WebSocket', MockWebSocket);
    terminalApiMocks.createTerminalMock.mockClear();
    terminalApiMocks.stopTerminalMock.mockClear();
    terminalApiMocks.getTerminalWebSocketUrlMock.mockClear();
    MockWebSocket.instances = [];
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

  it('stops terminal on unmount', async () => {
    const view = render(<Terminal terminalId="term-2" userId="user-1" />);
    await act(async () => {
      await Promise.resolve();
    });

    await act(async () => {
      view.unmount();
      await Promise.resolve();
    });

    expect(terminalApiMocks.stopTerminalMock).toHaveBeenCalledWith('term-2');
  });
});
