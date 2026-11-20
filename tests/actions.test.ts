import assert from 'node:assert/strict';
import { test } from 'node:test';
import { describeAction } from '../frontend/src/lib/actions.ts';

test('power actions read as plain language and only start is harmless', () => {
  assert.deepEqual(describeAction('power_server', { action: 'restart' }), {
    summary: 'Restart the server',
    details: [],
    danger: true,
  });
  assert.equal(describeAction('power_server', { action: 'start' }).danger, false);
  assert.equal(describeAction('power_server', { action: 'kill', server: '8c151efd' }).summary, 'Kill (force-stop) the server on server 8c151efd');
});

test('destructive file and backup actions are flagged', () => {
  const write = describeAction('write_file', { file: '/server.properties', content: 'a=1\nb=2\n' });
  assert.equal(write.summary, 'Write /server.properties (3 lines, replaces the whole file)');
  assert.equal(write.danger, true);
  assert.deepEqual(write.details, [{ label: 'New contents', text: 'a=1\nb=2\n' }]);

  assert.equal(describeAction('delete_files', { files: ['old.log', 'cache'], root: '/logs' }).summary, 'Delete old.log, cache in /logs');
  assert.equal(describeAction('delete_backup', { backup: 'b1' }).danger, true);
  assert.match(describeAction('restore_backup', { backup: 'b1', truncate_directory: true }).summary, /deleting all current files/);
  assert.equal(describeAction('edit_file', { file: '/a', old_string: 'x', new_string: 'y' }).danger, false);
});

test('settings changes list what they set', () => {
  assert.equal(
    describeAction('update_startup_variables', { variables: { MINECRAFT_VERSION: '1.21.4', MAX_PLAYERS: 20 } }).summary,
    'Set MINECRAFT_VERSION = 1.21.4, MAX_PLAYERS = 20 (takes effect after a restart)',
  );
  assert.equal(describeAction('rename_server', { name: 'Survival' }).summary, 'Rename the server to "Survival"');
  assert.equal(
    describeAction('rename_server', { name: 'Survival', description: 'PvE' }).summary,
    'Rename the server to "Survival" and change its description',
  );
  assert.equal(describeAction('rename_server', { description: '' }).summary, 'Remove its description');
});

test('raw API writes show method and path, DELETE is dangerous', () => {
  const remove = describeAction('write_api', { method: 'delete', path: '/api/admin/users/5f0e' });
  assert.equal(remove.summary, 'DELETE /api/admin/users/5f0e');
  assert.equal(remove.danger, true);
  assert.equal(describeAction('write_api', { method: 'POST', path: '/x', body: { a: 1 } }).details[0].label, 'Body');
});

test('unknown tools and broken arguments still produce a summary', () => {
  assert.equal(describeAction('new_tool', 'not an object').summary, 'Run new_tool');
  assert.equal(describeAction('create_backup', null).summary, 'Create a backup');
});
