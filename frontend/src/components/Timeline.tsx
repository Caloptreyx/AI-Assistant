import {
  faBan,
  faCheck,
  faChevronDown,
  faChevronRight,
  faCircleExclamation,
  faClockRotateLeft,
  faHand,
  faRobot,
  faRotateLeft,
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
import Alert from '@/elements/feedback/Alert.tsx';
import Collapse from '@/elements/layout/Collapse.tsx';
import Group from '@/elements/layout/Group.tsx';
import Paper from '@/elements/layout/Paper.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import Code from '@/elements/typography/Code.tsx';
import Text from '@/elements/typography/Text.tsx';
import type { RollbackActionResult, RollbackToResult } from '../api.ts';
import { describeAction } from '../lib/actions.ts';
import {
  type Action,
  type CallView,
  type ChatState,
  callView,
  canRollback,
  rollbackableToAction,
  type ToolCall,
} from '../lib/chat.ts';
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
  allActions,
  onDecide,
  onRollback,
  onRollbackTo,
  disabled,
}: {
  action: Action;
  allActions: Action[];
  onDecide: (action: Action, confirm: boolean) => Promise<void>;
  onRollback: (action: Action, force: boolean) => Promise<RollbackActionResult | null>;
  onRollbackTo: (action: Action, force: boolean) => Promise<RollbackToResult | null>;
  disabled: boolean;
}) {
  const { t: tExt } = useExtTranslations();
  const [deciding, setDeciding] = useState<'confirm' | 'reject' | null>(null);
  const [rolling, setRolling] = useState<'one' | 'to' | null>(null);
  const [conflict, setConflict] = useState<{ mode: 'one' | 'to'; message: string } | null>(null);
  const [raw, setRaw] = useState(false);
  const described = describeAction(action.tool, action.arguments);
  const status = STATUS[action.status];
  const pending = action.status === 'pending';
  const danger = described.danger && (pending || action.status === 'running');
  const rolledBack = action.rollback_status === 'rolled_back';
  const rollingBack = action.rollback_status === 'rolling_back' || rolling !== null;
  const reversible = canRollback(action);
  const plan = reversible ? rollbackableToAction(allActions, action.uuid) : null;

  const decide = async (confirm: boolean) => {
    setDeciding(confirm ? 'confirm' : 'reject');
    try {
      await onDecide(action, confirm);
    } finally {
      setDeciding(null);
    }
  };

  const runRollback = async (mode: 'one' | 'to', force: boolean) => {
    setRolling(mode);
    try {
      const result = mode === 'one' ? await onRollback(action, force) : await onRollbackTo(action, force);
      if (!result || result.ok) {
        setConflict(null);
      } else if (result.conflict) {
        setConflict({ mode, message: result.message });
      }
    } finally {
      setRolling(null);
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
          {rolledBack ? (
            <Badge
              color='violet'
              variant='light'
              leftSection={<FontAwesomeIcon icon={faRotateLeft} size='xs' />}
              className='shrink-0'
            >
              {tExt('action.rolledBack', {})}
            </Badge>
          ) : rollingBack ? (
            <Badge color='blue' variant='light' leftSection={<Loader size={10} color='blue' />} className='shrink-0'>
              {tExt('action.rollingBack', {})}
            </Badge>
          ) : (
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
          )}
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

        {action.rollback_result && (
          <Paper
            withBorder
            p='xs'
            radius='sm'
            className={action.rollback_status === 'failed' ? 'border-red-500/50!' : undefined}
          >
            <Text size='xs' c={action.rollback_status === 'failed' ? 'red' : 'dimmed'} mb={4}>
              {action.rollback_status === 'failed'
                ? tExt('action.rollbackFailed', {})
                : tExt('action.rollbackResult', {})}
            </Text>
            <Code block className={OUTPUT_CLASS}>
              {readable(action.rollback_result)}
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

        {reversible && (
          <Stack gap='xs'>
            {action.rollback_summary && (
              <Text size='xs' c='dimmed'>
                {action.rollback_summary}
              </Text>
            )}
            {conflict && (
              <Alert
                color='yellow'
                icon={<FontAwesomeIcon icon={faTriangleExclamation} />}
                title={tExt('action.rollbackConflictTitle', {})}
              >
                <Stack gap='xs'>
                  <Text size='sm'>{conflict.message}</Text>
                  <Group gap='xs'>
                    <Button
                      size='xs'
                      color='yellow'
                      leftSection={<FontAwesomeIcon icon={faRotateLeft} size='xs' />}
                      loading={rolling !== null}
                      disabled={disabled || rolling !== null}
                      onClick={() => runRollback(conflict.mode, true)}
                    >
                      {tExt('action.forceRollback', {})}
                    </Button>
                    <Button size='xs' variant='default' disabled={rolling !== null} onClick={() => setConflict(null)}>
                      {tExt('action.cancel', {})}
                    </Button>
                  </Group>
                </Stack>
              </Alert>
            )}
            <Group justify='flex-end' gap='xs' wrap='wrap'>
              {plan && plan.reversible.length > 1 && (
                <Button
                  size='xs'
                  variant='default'
                  leftSection={<FontAwesomeIcon icon={faClockRotateLeft} size='xs' />}
                  loading={rolling === 'to'}
                  disabled={disabled || rolling !== null}
                  onClick={() => runRollback('to', false)}
                >
                  {tExt('action.rollbackTo', { count: String(plan.reversible.length) })}
                </Button>
              )}
              <Button
                size='xs'
                variant='light'
                color='orange'
                leftSection={<FontAwesomeIcon icon={faRotateLeft} size='xs' />}
                loading={rolling === 'one'}
                disabled={disabled || rolling !== null}
                onClick={() => runRollback('one', false)}
              >
                {action.rollback_status === 'failed' ? tExt('action.retryRollback', {}) : tExt('action.rollback', {})}
              </Button>
            </Group>
            {plan && plan.reversible.length > 1 && plan.irreversible.length > 0 && (
              <Text size='xs' c='dimmed' ta='right'>
                {tExt('action.rollbackToSkippedHint', { count: String(plan.irreversible.length) })}
              </Text>
            )}
          </Stack>
        )}

        {action.status === 'succeeded' && action.rollback_status === 'unavailable' && (
          <Text size='xs' c='dimmed'>
            {action.rollback_summary ?? tExt('action.rollbackUnavailable', {})}
          </Text>
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
  onRollback,
  onRollbackTo,
  disabled,
}: {
  text: string;
  calls: ToolCall[];
  chat: ChatState;
  titles: Map<string, string>;
  onDecide: (action: Action, confirm: boolean) => Promise<void>;
  onRollback: (action: Action, force: boolean) => Promise<RollbackActionResult | null>;
  onRollbackTo: (action: Action, force: boolean) => Promise<RollbackToResult | null>;
  disabled: boolean;
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
          <ActionCard
            key={view.call.id}
            action={view.action}
            allActions={chat.actions}
            onDecide={onDecide}
            onRollback={onRollback}
            onRollbackTo={onRollbackTo}
            disabled={disabled}
          />
        ))}
      </Stack>
    </AssistantBlock>
  );
}

export default function Timeline({
  chat,
  titles,
  onDecide,
  onRollback,
  onRollbackTo,
  disabled = false,
}: {
  chat: ChatState;
  titles: Map<string, string>;
  onDecide: (action: Action, confirm: boolean) => Promise<void>;
  onRollback: (action: Action, force: boolean) => Promise<RollbackActionResult | null>;
  onRollbackTo: (action: Action, force: boolean) => Promise<RollbackToResult | null>;
  disabled?: boolean;
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
              onRollback={onRollback}
              onRollbackTo={onRollbackTo}
              disabled={chat.busy || disabled}
            />
          );
        }
        // Tool results show under the call they answer.
        return null;
      })}
    </Stack>
  );
}
