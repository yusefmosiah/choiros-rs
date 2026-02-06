import { describe, expect, it } from 'vitest';
import { parseChatStreamMessage, parseResponsePayload } from './ws';

describe('chat ws utils', () => {
  it('parses response stream message', () => {
    const msg = parseChatStreamMessage('{"type":"response","content":"{\\"text\\":\\"hi\\"}"}');
    expect(msg).toEqual({ type: 'response', content: '{"text":"hi"}' });
  });

  it('returns null for invalid stream payload', () => {
    expect(parseChatStreamMessage('bad-json')).toBeNull();
    expect(parseChatStreamMessage('{"ok":true}')).toBeNull();
    expect(parseChatStreamMessage('{"type":"response"}')).toBeNull();
    expect(parseChatStreamMessage('{"type":"error"}')).toBeNull();
  });

  it('extracts response payload from JSON content', () => {
    expect(
      parseResponsePayload(
        '{"text":"assistant reply","confidence":0.8,"model_used":"foo","client_message_id":"pending-1"}',
      ),
    ).toEqual({
      text: 'assistant reply',
      confidence: 0.8,
      model_used: 'foo',
      client_message_id: 'pending-1',
    });
  });

  it('falls back to raw content when not json', () => {
    expect(parseResponsePayload('plain text')).toEqual({ text: 'plain text' });
  });
});
