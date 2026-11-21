import assert from 'node:assert/strict';
import { test } from 'node:test';
import { chatShells, isChatPage } from '../frontend/src/lib/display.ts';

test('each display setting mounts its shells', () => {
  assert.deepEqual(chatShells('page'), { page: true, popup: false });
  assert.deepEqual(chatShells('popup'), { page: false, popup: true });
  assert.deepEqual(chatShells('both'), { page: true, popup: true });
});

test('a missing or unknown setting keeps the sidebar page, as before 0.1.1', () => {
  assert.deepEqual(chatShells(undefined), { page: true, popup: false });
  assert.deepEqual(chatShells('sidebar'), { page: true, popup: false });
  assert.deepEqual(chatShells(null), { page: true, popup: false });
});

test('the chat page is recognised for the server and the admin chat', () => {
  assert.equal(isChatPage('/server/abe26b3c/ai-assistant'), true);
  assert.equal(isChatPage('/admin/ai-assistant/'), true);
  assert.equal(isChatPage('/server/abe26b3c/console'), false);
  assert.equal(isChatPage('/server/abe26b3c/ai-assistant-logs'), false);
});
