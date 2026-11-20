import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  type Action,
  applyEvent,
  type ChatState,
  callView,
  canContinue,
  emptyChat,
  type Message,
  pendingActions,
  withMessage,
} from '../frontend/src/lib/chat.ts';

const call = { id: 'c1', name: 'read_console', arguments: { lines: 50 } };
const change = { id: 'c2', name: 'power_server', arguments: { action: 'restart' } };

function event(name: string, data: unknown) {
  return { event: name, data: JSON.stringify(data) };
}

function message(id: number, entry: Message['entry']): Message {
  return { id, entry, created: '2026-11-20T20:00:00Z' };
}

const action: Action = {
  uuid: 'a1',
  call_id: 'c2',
  tool: 'power_server',
  title: 'Start, stop, restart or kill the server',
  arguments: { action: 'restart' },
  status: 'pending',
  result: null,
  created: '2026-11-20T20:00:01Z',
  decided: null,
};

function play(events: { event: string; data: string }[]): ChatState {
  return events.reduce(applyEvent, { ...emptyChat, busy: true });
}

test('a streamed turn ends as stored messages, a running read and a pending action', () => {
  const state = play([
    event('message', message(1, { type: 'user', text: 'Restart it' })),
    event('delta', { text: 'Checking ' }),
    event('delta', { text: 'first.' }),
  ]);
  assert.equal(state.draft, 'Checking first.');

  const after = play([
    event('message', message(1, { type: 'user', text: 'Restart it' })),
    event('delta', { text: 'Checking first.' }),
    event('message', message(2, { type: 'assistant', text: 'Checking first.', tool_calls: [call, change] })),
    event('tool', { call_id: 'c1', name: 'read_console' }),
    event('tool', { call_id: 'c2', name: 'power_server' }),
    event('message', message(3, { type: 'tool_result', call_id: 'c1', name: 'read_console', content: 'Done', is_error: false })),
    event('action', action),
    event('done', { waiting: true }),
  ]);
  assert.equal(after.draft, '', 'the stored assistant message replaces the draft');
  assert.equal(after.busy, false);
  assert.deepEqual(after.running, []);
  assert.deepEqual(
    after.messages.map((stored) => stored.id),
    [1, 2, 3],
  );
  assert.deepEqual(callView(after, call), { kind: 'result', call, content: 'Done', isError: false });
  assert.equal(callView(after, change).kind, 'action');
  assert.deepEqual(pendingActions(after), [action]);
  assert.equal(canContinue(after), false, 'the pending action has to be decided first');
});

test('a read tool shows as running until its result arrives', () => {
  const state = play([
    event('message', message(2, { type: 'assistant', text: '', tool_calls: [call] })),
    event('tool', { call_id: 'c1', name: 'read_console' }),
  ]);
  assert.equal(callView(state, call).kind, 'running');
});

test('a decided action lets the model continue', () => {
  let state = play([
    event('message', message(2, { type: 'assistant', text: '', tool_calls: [change] })),
    event('action', action),
    event('done', { waiting: true }),
  ]);
  state = withMessage(
    { ...state, actions: [{ ...action, status: 'rejected', result: 'The user rejected this action.' }] },
    message(3, { type: 'tool_result', call_id: 'c2', name: 'power_server', content: 'The user rejected this action.', is_error: true }),
  );
  assert.equal(pendingActions(state).length, 0);
  assert.equal(canContinue(state), true);
});

test('errors are kept and broken events are ignored', () => {
  const state = play([
    { event: 'delta', data: '{not json' },
    event('error', { message: 'The AI provider answered HTTP 401: invalid x-api-key' }),
    event('done', { waiting: false }),
  ]);
  assert.equal(state.error, 'The AI provider answered HTTP 401: invalid x-api-key');
  assert.equal(state.draft, '');
});
