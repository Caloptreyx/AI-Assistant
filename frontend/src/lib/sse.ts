// Server-sent events from a fetch() response. EventSource only does GET, and a chat turn is a
// POST with the message as its body, so the stream is read by hand.

export interface SseMessage {
  event: string;
  data: string;
}

export class SseParser {
  private buffer = '';
  private event = '';
  private data: string[] = [];

  /** Takes the next piece of the stream and returns the events it completed. */
  push(chunk: string): SseMessage[] {
    this.buffer += chunk;
    const messages: SseMessage[] = [];
    let newline = this.buffer.indexOf('\n');
    while (newline !== -1) {
      let line = this.buffer.slice(0, newline);
      this.buffer = this.buffer.slice(newline + 1);
      if (line.endsWith('\r')) {
        line = line.slice(0, -1);
      }
      const message = this.line(line);
      if (message) {
        messages.push(message);
      }
      newline = this.buffer.indexOf('\n');
    }
    return messages;
  }

  /** The event still being collected when the stream ended without a blank line. */
  finish(): SseMessage[] {
    const messages = this.buffer ? this.push('\n') : [];
    const last = this.dispatch();
    return last ? [...messages, last] : messages;
  }

  private line(line: string): SseMessage | null {
    if (line === '') {
      return this.dispatch();
    }
    if (line.startsWith(':')) {
      return null;
    }
    const colon = line.indexOf(':');
    const field = colon === -1 ? line : line.slice(0, colon);
    let value = colon === -1 ? '' : line.slice(colon + 1);
    if (value.startsWith(' ')) {
      value = value.slice(1);
    }
    if (field === 'event') {
      this.event = value;
    } else if (field === 'data') {
      this.data.push(value);
    }
    return null;
  }

  private dispatch(): SseMessage | null {
    const event = this.event || 'message';
    this.event = '';
    if (this.data.length === 0) {
      return null;
    }
    const data = this.data.join('\n');
    this.data = [];
    return { event, data };
  }
}

/** Reads a whole event stream, calling `onMessage` for every event as it arrives. */
export async function readSse(
  stream: ReadableStream<Uint8Array>,
  onMessage: (message: SseMessage) => void,
): Promise<void> {
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  const parser = new SseParser();
  for (;;) {
    const { done, value } = await reader.read();
    if (done) {
      break;
    }
    for (const message of parser.push(decoder.decode(value, { stream: true }))) {
      onMessage(message);
    }
  }
  for (const message of parser.push(decoder.decode())) {
    onMessage(message);
  }
  for (const message of parser.finish()) {
    onMessage(message);
  }
}
