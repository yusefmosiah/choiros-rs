// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Chat } from './Chat';
import { useChatStore } from '@/stores/chat';

const chatApiMocks = vi.hoisted(() => ({
  getMessagesMock: vi.fn(async () => []),
  sendMessageMock: vi.fn(async () => 'temp-id'),
}));

vi.mock('@/lib/api/chat', () => ({
  getMessages: chatApiMocks.getMessagesMock,
  sendMessage: chatApiMocks.sendMessageMock,
}));

class MockWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;
  static instances: MockWebSocket[] = [];

  public readyState = MockWebSocket.CONNECTING;
  public onopen: ((event: Event) => void) | null = null;
  public onmessage: ((event: MessageEvent) => void) | null = null;
  public onerror: ((event: Event) => void) | null = null;
  public onclose: ((event: Event) => void) | null = null;
  public send = vi.fn<(data: string) => void>();
  public close = vi.fn(() => {
    this.readyState = MockWebSocket.CLOSED;
  });

  constructor(public url: string) {
    MockWebSocket.instances.push(this);
  }

  emitOpen() {
    this.readyState = MockWebSocket.OPEN;
    this.onopen?.(new Event('open'));
  }

  emitMessage(data: string) {
    this.onmessage?.({ data } as MessageEvent);
  }
}

describe('Chat component websocket lifecycle', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.stubGlobal('WebSocket', MockWebSocket);
    useChatStore.getState().clear();
    chatApiMocks.getMessagesMock.mockClear();
    chatApiMocks.sendMessageMock.mockClear();
    MockWebSocket.instances = [];
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it('resolves the matching pending message by client_message_id', async () => {
    render(<Chat actorId="actor-1" userId="user-1" />);

    expect(MockWebSocket.instances).toHaveLength(1);
    const ws = MockWebSocket.instances[0];
    await act(async () => {
      ws.emitOpen();
    });

    const textarea = screen.getByPlaceholderText('Type a message...');
    fireEvent.change(textarea, { target: { value: 'hello' } });
    fireEvent.click(screen.getByText('Send'));

    const firstSendPayload = JSON.parse(ws.send.mock.calls[0][0]) as { client_message_id?: string };
    expect(firstSendPayload.client_message_id).toBeTruthy();

    await act(async () => {
      ws.emitMessage(
        JSON.stringify({
          type: 'response',
          content: JSON.stringify({
            text: 'assistant reply',
            client_message_id: firstSendPayload.client_message_id,
          }),
        }),
      );
    });

    expect(document.querySelectorAll('.chat-msg--pending').length).toBe(0);
  });

  it('times out pending websocket messages after 20s', async () => {
    render(<Chat actorId="actor-1" userId="user-1" />);

    const ws = MockWebSocket.instances[0];
    await act(async () => {
      ws.emitOpen();
    });

    const textarea = screen.getByPlaceholderText('Type a message...');
    fireEvent.change(textarea, { target: { value: 'will timeout' } });
    fireEvent.click(screen.getByText('Send'));

    expect(document.querySelectorAll('.chat-msg--pending').length).toBe(1);

    await act(async () => {
      vi.advanceTimersByTime(20_000);
    });

    expect(document.querySelectorAll('.chat-msg--pending').length).toBe(0);
    expect(screen.getByText(/Assistant response timeout/)).toBeTruthy();
  });
});
