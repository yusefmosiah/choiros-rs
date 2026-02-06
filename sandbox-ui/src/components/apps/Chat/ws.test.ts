import { describe, expect, it } from 'vitest';
import { parseChatStreamMessage, parseResponseText } from './ws';

describe('chat ws utils', () => {
  it('parses response stream message', () => {
    const msg = parseChatStreamMessage('{"type":"response","content":"{\\"text\\":\\"hi\\"}"}');
    expect(msg).toEqual({ type: 'response', content: '{"text":"hi"}' });
  });

  it('returns null for invalid stream payload', () => {
    expect(parseChatStreamMessage('bad-json')).toBeNull();
    expect(parseChatStreamMessage('{"ok":true}')).toBeNull();
  });

  it('extracts text from JSON response content', () => {
    expect(parseResponseText('{"text":"assistant reply"}')).toBe('assistant reply');
  });

  it('falls back to raw content when not json', () => {
    expect(parseResponseText('plain text')).toBe('plain text');
  });
});
