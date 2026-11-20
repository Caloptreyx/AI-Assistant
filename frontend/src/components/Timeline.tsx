import {
  faBan,
  faCheck,
  faChevronDown,
  faChevronRight,
  faCircleExclamation,
  faHand,
  faRobot,
  faTriangleExclamation,
  faXmark,
} from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { Loader } from '@mantine/core';
import { type ReactNode, useState } from 'react';
import Button from '@/elements/buttons/Button.tsx';
import Badge from '@/elements/data-display/Badge.tsx';
import Card from '@/elements/data-display/Card.tsx';
import ThemeIcon from '@/elements/data-display/ThemeIcon.tsx';
import Collapse from '@/elements/layout/Collapse.tsx';
import Group from '@/elements/layout/Group.tsx';
import Paper from '@/elements/layout/Paper.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import Code from '@/elements/typography/Code.tsx';
import Text from '@/elements/typography/Text.tsx';
import { describeAction } from '../lib/actions.ts';
import { type Action, type CallView, type ChatState, callView, type ToolCall } from '../lib/chat.ts';
import { useExtTranslations } from '../translations.ts';
import ChatMarkdown from './Markdown.tsx';

/** Panel responses are one long JSON line; indent them so they wrap readably. */
function readable(output: string): string {
  try {
    return JSON.stringify(JSON.parse(output), null, 2);
  } catch {
    return output;
  }
}

const OUTPUT_CLASS = 'max-h-72 overflow-auto whitespace-pre-wrap break-words text-xs';

export function AssistantAvatar() {
  return (
    <ThemeIcon variant='light' radius='md' size='md' className='mt-0.5 shrink-0'>
      <FontAwesomeIcon icon={faRobot} size='sm' />
    </ThemeIcon>
  );
}

/** The model's answer, with an avatar so it reads as the assistant speaking. */
export function AssistantBlock({ children }: { children: ReactNode }) {
  return (
    <Group align='flex-start' gap='sm' wrap='nowrap'>
      <AssistantAvatar />
      <div className='min-w-0 flex-1'>{children}</div>
    </Group>
  );
}

function UserMessage({ text }: { text: string }) {
  return (
    <Group justify='flex-end'>
      <Paper
        p='sm'
        radius='lg'
        className='max-w-[85%] whitespace-pre-wrap break-words rounded-br-sm text-sm'
        bg='var(--mantine-primary-color-light)'
      >
        {text}
      </Paper>
    </Group>
  );
}

const STATUS: Record<Action['status'], { color: string; icon: typeof faCheck | null }> = {
  pending: { color: 'yellow', icon: faHand },
  running: { color: 'blue', icon: null },
  succeeded: { color: 'green', icon: faCheck },
  failed: { color: 'red', icon: faCircleExclamation },
  rejected: { color: 'gray', icon: faBan },
};

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
  const [raw, setRaw] = useState(false);
  const described = describeAction(action.tool, action.arguments);
  const status = STATUS[action.status];
  const pending = action.status === 'pending';
  const danger = described.danger && (pending || action.status === 'running');

  const decide = async (confirm: boolean) => {
    setDeciding(confirm ? 'confirm' : 'reject');
    try {
      await onDecide(action, confirm);
    } finally {
      setDeciding(null);
    }
  };

  return (
    <Card
      withBorder
      p='md'
      radius='md'
      className={pending ? (danger ? 'border-red-500/60!' : 'border-yellow-500/50!') : undefined}
    >
      <Stack gap='sm'>
        <Group justify='space-between' wrap='nowrap' align='flex-start'>
          <div className='min-w-0'>
            <Text size='xs' c='dimmed' tt='uppercase' fw={600} className='tracking-wide'>
              {action.title}
            </Text>
            <Text fw={600} className='break-words'>
              {described.summary}
            </Text>
          </div>
          <Badge
            color={status.color}
            variant='light'
            leftSection={
              status.icon ? <FontAwesomeIcon icon={status.icon} size='xs' /> : <Loader size={10} color='blue' />
            }
            className='shrink-0'
          >
            {tExt(`action.${action.status}`, {})}
          </Badge>
        </Group>

        {danger && (
          <Group gap='xs' wrap='nowrap'>
            <Text c='red' size='sm'>
              <FontAwesomeIcon icon={faTriangleExclamation} />
            </Text>
            <Text c='red' size='sm'>
              {tExt('action.danger', {})}
            </Text>
          </Group>
        )}

        {described.details.map((detail) => (
          <div key={detail.label}>
            <Text size='xs' c='dimmed' mb={4}>
              {detail.label}
            </Text>
            <Code block className={OUTPUT_CLASS}>
              {detail.text}
            </Code>
          </div>
        ))}

        <div>
          <Button
            variant='subtle'
            size='compact-xs'
            color='gray'
            leftSection={<FontAwesomeIcon icon={raw ? faChevronDown : faChevronRight} size='xs' />}
            onClick={() => setRaw((value) => !value)}
          >
            {tExt('action.raw', {})}
          </Button>
          <Collapse expanded={raw}>
            <Code block className={`mt-1 ${OUTPUT_CLASS}`}>
              {JSON.stringify(action.arguments, null, 2)}
            </Code>
          </Collapse>
        </div>

        {action.result && !pending && (
          <Paper withBorder p='xs' radius='sm'>
            <Text size='xs' c='dimmed' mb={4}>
              {tExt('action.result', {})}
            </Text>
            <Code block className={OUTPUT_CLASS}>
              {readable(action.result)}
            </Code>
          </Paper>
        )}

        {pending && (
          <Group justify='flex-end' gap='xs'>
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
              color={danger ? 'red' : 'green'}
              leftSection={<FontAwesomeIcon icon={faCheck} />}
              onClick={() => decide(true)}
              loading={deciding === 'confirm'}
              disabled={disabled || deciding !== null}
            >
              {danger ? tExt('action.confirmDanger', {}) : tExt('action.confirm', {})}
            </Button>
          </Group>
        )}
      </Stack>
    </Card>
  );
}

function ToolCalls({ views, titles }: { views: Exclude<CallView, { kind: 'action' }>[]; titles: Map<string, string> }) {
  const { t: tExt } = useExtTranslations();
  const [open, setOpen] = useState<string | null>(null);
  const opened = views.find((view) => view.call.id === open);

  return (
    <div>
      <Group gap={6}>
        {views.map((view) => {
          const title = titles.get(view.call.name) ?? view.call.name;
          const failed = view.kind === 'result' && view.isError;
          return (
            <Button
              key={view.call.id}
              variant={open === view.call.id ? 'light' : 'default'}
              size='compact-xs'
              radius='xl'
              color={failed ? 'red' : 'gray'}
              disabled={view.kind !== 'result'}
              leftSection={
                view.kind === 'running' ? (
                  <Loader size={10} />
                ) : (
                  <FontAwesomeIcon icon={failed || view.kind === 'missing' ? faCircleExclamation : faCheck} size='xs' />
                )
              }
              onClick={() => setOpen((current) => (current === view.call.id ? null : view.call.id))}
            >
              {view.kind === 'running' ? tExt('tool.running', { title }) : title}
            </Button>
          );
        })}
      </Group>
      <Collapse expanded={opened?.kind === 'result'}>
        {opened?.kind === 'result' && (
          <Code block className={`mt-2 ${OUTPUT_CLASS}`}>
            {readable(opened.content)}
          </Code>
        )}
      </Collapse>
    </div>
  );
}

function AssistantMessage({
  text,
  calls,
  chat,
  titles,
  onDecide,
}: {
  text: string;
  calls: ToolCall[];
  chat: ChatState;
  titles: Map<string, string>;
  onDecide: (action: Action, confirm: boolean) => Promise<void>;
}) {
  const views = calls.map((call) => callView(chat, call));
  const tools = views.filter((view): view is Exclude<CallView, { kind: 'action' }> => view.kind !== 'action');
  const actions = views.filter((view): view is Extract<CallView, { kind: 'action' }> => view.kind === 'action');

  return (
    <AssistantBlock>
      <Stack gap='sm'>
        {text.trim() && <ChatMarkdown text={text} />}
        {tools.length > 0 && <ToolCalls views={tools} titles={titles} />}
        {actions.map((view) => (
          <ActionCard key={view.call.id} action={view.action} onDecide={onDecide} disabled={chat.busy} />
        ))}
      </Stack>
    </AssistantBlock>
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
    <Stack gap='lg'>
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
            />
          );
        }
        // Tool results show under the call they answer.
        return null;
      })}
    </Stack>
  );
}
