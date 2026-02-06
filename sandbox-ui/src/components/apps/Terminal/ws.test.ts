import { describe, expect, it } from 'vitest';
import { parseTerminalWsMessage, reconnectDelayMs } from './ws';

describe('terminal ws utils', () => {
  it('parses a valid output message', () => {
    const msg = parseTerminalWsMessage('{"type":"output","data":"hello"}');
    expect(msg).toEqual({ type: 'output', data: 'hello' });
  });

  it('returns null for invalid payload', () => {
    expect(parseTerminalWsMessage('not-json')).toBeNull();
    expect(parseTerminalWsMessage('{"foo":1}')).toBeNull();
  });

  it('calculates exponential reconnect delay with max cap', () => {
    expect(reconnectDelayMs(0)).toBe(500);
    expect(reconnectDelayMs(1)).toBe(1000);
    expect(reconnectDelayMs(2)).toBe(2000);
    expect(reconnectDelayMs(20)).toBe(8000);
  });
});
