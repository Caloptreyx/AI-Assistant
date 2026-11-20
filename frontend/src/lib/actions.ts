// Plain-language summaries of the changes the assistant proposes, for the action cards.
// Free of React and panel imports so `node --test` can check them.

export interface ActionSummary {
  /** One line saying what will change, e.g. `Restart the server`. */
  summary: string;
  /** Longer content worth showing on its own (file contents, a replacement, a request body). */
  details: { label: string; text: string }[];
  /** Deletes, overwrites or interrupts something that cannot simply be undone. */
  danger: boolean;
}

type Args = Record<string, unknown>;

const text = (value: unknown): string => (typeof value === 'string' ? value : JSON.stringify(value ?? ''));
const quoted = (value: unknown): string => `"${text(value)}"`;
const plural = (count: number, word: string): string => `${count} ${word}${count === 1 ? '' : 's'}`;

function onServer(args: Args): string {
  return typeof args.server === 'string' && args.server ? ` on server ${args.server}` : '';
}

export function describeAction(tool: string, raw: unknown): ActionSummary {
  const args: Args = raw && typeof raw === 'object' && !Array.isArray(raw) ? (raw as Args) : {};
  const where = onServer(args);
  const plain = (summary: string, danger = false): ActionSummary => ({ summary, details: [], danger });

  switch (tool) {
    case 'power_server': {
      const action = text(args.action);
      const verbs: Record<string, string> = {
        start: 'Start',
        stop: 'Stop',
        restart: 'Restart',
        kill: 'Kill (force-stop)',
      };
      return plain(`${verbs[action] ?? action} the server${where}`, action !== 'start');
    }
    case 'send_command':
    case 'run_command':
      return {
        summary: `Run a console command${where}`,
        details: [{ label: 'Command', text: text(args.command) }],
        danger: false,
      };
    case 'rename_server': {
      const parts: string[] = [];
      if (typeof args.name === 'string' && args.name) parts.push(`rename the server to ${quoted(args.name)}`);
      if (typeof args.description === 'string') {
        parts.push(args.description ? 'change its description' : 'remove its description');
      }
      const summary = parts.join(' and ') || 'rename the server';
      return plain(`${summary.charAt(0).toUpperCase()}${summary.slice(1)}${where}`);
    }
    case 'write_file':
      return {
        summary: `Write ${text(args.file)} (${plural(text(args.content).split('\n').length, 'line')}, replaces the whole file)${where}`,
        details: [{ label: 'New contents', text: text(args.content) }],
        danger: true,
      };
    case 'edit_file':
      return {
        summary: `Edit ${text(args.file)}${args.replace_all ? ' (every occurrence)' : ''}${where}`,
        details: [
          { label: 'Replace', text: text(args.old_string) },
          { label: 'With', text: text(args.new_string) },
        ],
        danger: false,
      };
    case 'delete_files': {
      const files = Array.isArray(args.files) ? args.files.map(text) : [];
      const root = typeof args.root === 'string' && args.root ? args.root : '/';
      return plain(`Delete ${files.join(', ') || 'files'} in ${root}${where}`, true);
    }
    case 'create_backup':
      return plain(`Create a backup${args.name ? ` named ${quoted(args.name)}` : ''}${where}`);
    case 'restore_backup':
      return plain(
        `Restore backup ${text(args.backup)}${args.truncate_directory ? ', deleting all current files first' : ', overwriting changed files'}${where}`,
        true,
      );
    case 'delete_backup':
      return plain(`Delete backup ${text(args.backup)} permanently${where}`, true);
    case 'create_schedule': {
      const steps = Array.isArray(args.steps) ? args.steps.length : 0;
      return plain(
        `Create schedule ${quoted(args.name)}${args.cron ? ` (cron ${text(args.cron)})` : ''} with ${plural(steps, 'step')}${where}`,
      );
    }
    case 'run_schedule':
      return plain(`Run schedule ${text(args.schedule)} now${where}`);
    case 'update_startup_variables': {
      const variables =
        args.variables && typeof args.variables === 'object' ? Object.entries(args.variables as Args) : [];
      return plain(
        `Set ${variables.map(([name, value]) => `${name} = ${text(value)}`).join(', ') || 'startup variables'}${where} (takes effect after a restart)`,
      );
    }
    case 'write_api': {
      const method = text(args.method).toUpperCase();
      const body = args.body === undefined ? [] : [{ label: 'Body', text: JSON.stringify(args.body, null, 2) }];
      return { summary: `${method} ${text(args.path)}`, details: body, danger: method === 'DELETE' };
    }
    default:
      return plain(`Run ${tool}${where}`);
  }
}
