import {
  faCheck,
  faChevronDown,
  faChevronRight,
  faCircleExclamation,
  faHand,
  faSpinner,
  faXmark,
} from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { Typography } from '@mantine/core';
import { type ReactNode, useState } from 'react';
import Markdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import Button from '@/elements/buttons/Button.tsx';
import Badge from '@/elements/data-display/Badge.tsx';
import Card from '@/elements/data-display/Card.tsx';
import Group from '@/elements/layout/Group.tsx';
import Paper from '@/elements/layout/Paper.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import Code from '@/elements/typography/Code.tsx';
import Text from '@/elements/typography/Text.tsx';
import { markdownComponents } from '@/providers/TranslationProvider.tsx';
import { type Action, type CallView, type ChatState, callView, type ToolCall } from '../lib/chat.ts';
import { useExtTranslations } from '../translations.ts';

const chatMarkdown = {
  ...markdownComponents,
  // The panel's translation markdown renders paragraphs inline; chat answers need real ones.
  p: ({ children }: { children?: ReactNode }) => <p>{children}</p>,
};

export function AssistantText({ text }: { text: string }) {
  return (
    <Typography>
      <Markdown remarkPlugins={[remarkGfm]} components={chatMarkdown}>
        {text}
      </Markdown>
    </Typography>
  );
}

function UserMessage({ text }: { text: string }) {
  return (
    <Group justify='flex-end'>
      <Paper withBorder p='sm' radius='md' className='max-w-[80%] whitespace-pre-wrap break-words'>
        {text}
      </Paper>
    </Group>
  );
}

const ACTION_COLORS: Record<Action['status'], string> = {
  pending: 'yellow',
  running: 'blue',
  succeeded: 'green',
  failed: 'red',
  rejected: 'gray',
};

/** Long text arguments (file contents, replacements) shown as their own blocks. */
const TEXT_ARGUMENTS = ['content', 'old_string', 'new_string', 'command', 'body'];

/** Panel responses are one long JSON line; indent them so they wrap readably. */
function readable(output: string): string {
  try {
    return JSON.stringify(JSON.parse(output), null, 2);
  } catch {
    return output;
  }
}

const OUTPUT_CLASS = 'max-h-80 overflow-auto whitespace-pre-wrap break-words';

function ActionArguments({ value }: { value: unknown }) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return <Code block>{JSON.stringify(value, null, 2)}</Code>;
  }
  const entries = Object.entries(value as Record<string, unknown>);
  const blocks = entries.filter(([key, item]) => TEXT_ARGUMENTS.includes(key) && typeof item === 'string');
  const rest = Object.fromEntries(entries.filter(([key]) => !blocks.some(([block]) => block === key)));

  return (
    <Stack gap='xs'>
      {Object.keys(rest).length > 0 && <Code block>{JSON.stringify(rest, null, 2)}</Code>}
      {blocks.map(([key, item]) => (
        <div key={key}>
          <Text size='xs' c='dimmed'>
            {key}
          </Text>
          <Code block className={OUTPUT_CLASS}>
            {String(item)}
          </Code>
        </div>
      ))}
    </Stack>
  );
}

function ActionCard({
  action,
  onDecide,
  disabled,
}: {
  action: Action;
  onDecide: (action: Action, confirm: boolean) => Promise<void>;
  disabled: boolean;
}) {
  const { t: tExt } = useExtTranslations();
  const [deciding, setDeciding] = useState<'confirm' | 'reject' | null>(null);

  const decide = async (confirm: boolean) => {
    setDeciding(confirm ? 'confirm' : 'reject');
    try {
      await onDecide(action, confirm);
    } finally {
      setDeciding(null);
    }
  };

  return (
    <Card withBorder p='md'>
      <Stack gap='sm'>
        <Group justify='space-between' wrap='nowrap'>
          <Group gap='xs' wrap='nowrap'>
            <FontAwesomeIcon icon={faHand} />
            <Text fw={600}>{action.title}</Text>
          </Group>
          <Badge color={ACTION_COLORS[action.status]} variant='light'>
            {tExt(`action.${action.status}`, {})}
          </Badge>
        </Group>
        <Text size='xs' c='dimmed'>
          {tExt('action.details', {})}
        </Text>
        <ActionArguments value={action.arguments} />
        {action.result && action.status !== 'pending' && (
          <Code block className={OUTPUT_CLASS}>
            {readable(action.result)}
          </Code>
        )}
        {action.status === 'pending' && (
          <Group justify='flex-end'>
            <Button
              variant='default'
              leftSection={<FontAwesomeIcon icon={faXmark} />}
              onClick={() => decide(false)}
              loading={deciding === 'reject'}
              disabled={disabled || deciding !== null}
            >
              {tExt('action.reject', {})}
            </Button>
            <Button
              color='green'
              leftSection={<FontAwesomeIcon icon={faCheck} />}
              onClick={() => decide(true)}
              loading={deciding === 'confirm'}
              disabled={disabled || deciding !== null}
            >
              {tExt('action.confirm', {})}
            </Button>
          </Group>
        )}
      </Stack>
    </Card>
  );
}

function ToolCallItem({ view, title }: { view: Exclude<CallView, { kind: 'action' }>; title: string }) {
  const { t: tExt } = useExtTranslations();
  const [open, setOpen] = useState(false);

  if (view.kind === 'running') {
    return (
      <Text size='sm' c='dimmed'>
        <FontAwesomeIcon icon={faSpinner} spin /> {tExt('tool.running', { title })}
      </Text>
    );
  }
  if (view.kind === 'missing') {
    return (
      <Text size='sm' c='dimmed'>
        <FontAwesomeIcon icon={faCircleExclamation} /> {tExt('tool.missing', { title })}
      </Text>
    );
  }

  return (
    <div>
      <Button
        variant='subtle'
        size='compact-sm'
        color={view.isError ? 'red' : 'gray'}
        leftSection={<FontAwesomeIcon icon={open ? faChevronDown : faChevronRight} />}
        onClick={() => setOpen((value) => !value)}
      >
        {view.isError ? tExt('tool.failed', { title }) : tExt('tool.done', { title })}
      </Button>
      {open && (
        <Code block className={`mt-1 ${OUTPUT_CLASS}`}>
          {readable(view.content)}
        </Code>
      )}
    </div>
  );
}

function AssistantMessage({
  text,
  calls,
  chat,
  titles,
  onDecide,
  busy,
}: {
  text: string;
  calls: ToolCall[];
  chat: ChatState;
  titles: Map<string, string>;
  onDecide: (action: Action, confirm: boolean) => Promise<void>;
  busy: boolean;
}) {
  return (
    <Stack gap='xs'>
      {text.trim() && <AssistantText text={text} />}
      {calls.map((call) => {
        const view = callView(chat, call);
        return view.kind === 'action' ? (
          <ActionCard key={call.id} action={view.action} onDecide={onDecide} disabled={busy} />
        ) : (
          <ToolCallItem key={call.id} view={view} title={titles.get(call.name) ?? call.name} />
        );
      })}
    </Stack>
  );
}

export default function Timeline({
  chat,
  titles,
  onDecide,
}: {
  chat: ChatState;
  titles: Map<string, string>;
  onDecide: (action: Action, confirm: boolean) => Promise<void>;
}) {
  return (
    <Stack gap='md'>
      {chat.messages.map(({ id, entry }) => {
        if (entry.type === 'user') {
          return <UserMessage key={id} text={entry.text} />;
        }
        if (entry.type === 'assistant') {
          return (
            <AssistantMessage
              key={id}
              text={entry.text}
              calls={entry.tool_calls}
              chat={chat}
              titles={titles}
              onDecide={onDecide}
              busy={chat.busy}
            />
          );
        }
        // Tool results show under the call they answer.
        return null;
      })}
    </Stack>
  );
}
