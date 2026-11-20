import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readSse, SseParser, type SseMessage } from '../frontend/src/lib/sse.ts';

// What the backend sends for one short turn (axum's Sse writer: `event:`/`data:` lines, a blank
// line after each event, `:` comments as keep-alives).
const TURN = [
  'event: message',
  'data: {"id":7,"entry":{"type":"user","text":"Is my server up?"},"created":"2026-11-20T20:00:00Z"}',
  '',
  ':',
  '',
  'event: delta',
  'data: {"text":"Let me "}',
  '',
  'event: delta',
  'data: {"text":"check – ✓"}',
  '',
  'event: done',
  'data: {"waiting":false}',
  '',
  '',
].join('\n');

function parseInChunks(input: string, size: number): SseMessage[] {
  const parser = new SseParser();
  const messages: SseMessage[] = [];
  for (let start = 0; start < input.length; start += size) {
    messages.push(...parser.push(input.slice(start, start + size)));
  }
  messages.push(...parser.finish());
  return messages;
}

test('events come out whole whatever the chunk size', () => {
  const expected = [
    {
      event: 'message',
      data: '{"id":7,"entry":{"type":"user","text":"Is my server up?"},"created":"2026-11-20T20:00:00Z"}',
    },
    { event: 'delta', data: '{"text":"Let me "}' },
    { event: 'delta', data: '{"text":"check – ✓"}' },
    { event: 'done', data: '{"waiting":false}' },
  ];
  for (const size of [1, 2, 5, 17, 64, TURN.length]) {
    assert.deepEqual(parseInChunks(TURN, size), expected, `chunk size ${size}`);
  }
});

test('CRLF line endings, multi-line data and a missing final blank line', () => {
  const messages = parseInChunks('data: one\r\ndata: two\r\n\r\nevent: done\r\ndata: {}', 3);
  assert.deepEqual(messages, [
    { event: 'message', data: 'one\ntwo' },
    { event: 'done', data: '{}' },
  ]);
});

test('readSse decodes UTF-8 split across network chunks', async () => {
  const bytes = new TextEncoder().encode(TURN);
  const byteOffset = (text: string) => new TextEncoder().encode(TURN.slice(0, TURN.indexOf(text))).length;
  // Cut inside the multi-byte "–" (3 bytes) and "✓" (3 bytes).
  const cuts = [0, 1, 150, byteOffset('–') + 1, byteOffset('✓') + 2, bytes.length];
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      for (let index = 1; index < cuts.length; index++) {
        controller.enqueue(bytes.slice(cuts[index - 1], cuts[index]));
      }
      controller.close();
    },
  });
  const messages: SseMessage[] = [];
  await readSse(stream, (message) => messages.push(message));
  assert.equal(messages.length, 4);
  assert.equal(JSON.parse(messages[2].data).text, 'check – ✓');
});
