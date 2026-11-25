import {
  faArrowDown,
  faBars,
  faKey,
  faPaperPlane,
  faRobot,
  faRotateRight,
  faStop,
} from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { Loader } from '@mantine/core';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { type KeyboardEvent, type ReactNode, useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate } from 'react-router';
import { httpErrorToHuman } from '@/api/axios.ts';
import ActionIcon from '@/elements/buttons/ActionIcon.tsx';
import Button from '@/elements/buttons/Button.tsx';
import Badge from '@/elements/data-display/Badge.tsx';
import Card from '@/elements/data-display/Card.tsx';
import ThemeIcon from '@/elements/data-display/ThemeIcon.tsx';
import Alert from '@/elements/feedback/Alert.tsx';
import TextArea from '@/elements/input/TextArea.tsx';
import Group from '@/elements/layout/Group.tsx';
import ScrollArea from '@/elements/layout/ScrollArea.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import Drawer from '@/elements/overlays/Drawer.tsx';
import Tooltip from '@/elements/overlays/Tooltip.tsx';
import Text from '@/elements/typography/Text.tsx';
import Title from '@/elements/typography/Title.tsx';
import { useToast } from '@/providers/ToastProvider.tsx';
import {
  type ChatInfo,
  type Conversation,
  createConversation,
  decideAction,
  deleteConversation,
  getConversation,
  getConversations,
  getInfo,
  PACKAGE,
  renameConversation,
  type RollbackActionResult,
  type RollbackToResult,
  rollbackAction,
  rollbackToAction,
  stopTurn,
  streamTurn,
} from '../api.ts';
import {
  type Action,
  applyEvent,
  type ChatState,
  canContinue,
  emptyChat,
  loadedChat,
  pendingActions,
  upsertAction,
  upsertActions,
  withMessage,
} from '../lib/chat.ts';
import { useExtTranslations } from '../translations.ts';
import ConversationList from './ConversationList.tsx';
import ChatMarkdown from './Markdown.tsx';
import Timeline, { AssistantBlock } from './Timeline.tsx';

const ACCOUNT_KEY_PATH = '/account/ai-assistant';
/** How close to the bottom (px) still counts as "following" the conversation. */
const STICK_DISTANCE = 96;

const SERVER_PROMPTS = ['server1', 'server2', 'server3', 'server4'] as const;
const ADMIN_PROMPTS = ['admin1', 'admin2', 'admin3', 'admin4'] as const;

function EmptyState({
  admin,
  onPick,
  disabled,
  compact,
}: {
  admin: boolean;
  onPick: (text: string) => void;
  disabled: boolean;
  compact: boolean;
}) {
  const { t: tExt } = useExtTranslations();
  const prompts = (admin ? ADMIN_PROMPTS : SERVER_PROMPTS).map((key) => tExt(`suggestions.${key}`, {}));

  return (
    <Stack align='center' gap='md' py={compact ? 'md' : 'xl'} className='mx-auto max-w-xl text-center'>
      <ThemeIcon variant='light' size={48} radius='md'>
        <FontAwesomeIcon icon={faRobot} size='lg' />
      </ThemeIcon>
      <div>
        <Title order={4}>{admin ? tExt('empty.adminTitle', {}) : tExt('empty.title', {})}</Title>
        <Text c='dimmed' size='sm' mt={4} className='text-balance'>
          {tExt('empty.body', {})}
        </Text>
      </div>
      <div className={`grid w-full grid-cols-1 gap-2 ${compact ? '' : 'sm:grid-cols-2'}`}>
        {prompts.map((prompt) => (
          <Button
            key={prompt}
            variant='default'
            onClick={() => onPick(prompt)}
            disabled={disabled}
            styles={{ inner: { justifyContent: 'flex-start' }, label: { whiteSpace: 'normal', textAlign: 'left' } }}
            className='h-auto! py-2!'
          >
            {prompt}
          </Button>
        ))}
      </div>
    </Stack>
  );
}

/** Why the chat cannot be used, with the step that fixes it. */
function Notice({ info }: { info: ChatInfo }) {
  const { t: tExt } = useExtTranslations();
  const navigate = useNavigate();

  if (!info.enabled) {
    return <Alert color='yellow'>{tExt('chat.disabled', {})}</Alert>;
  }
  if (info.access.needs_user_key) {
    return (
      <Alert color='blue' icon={<FontAwesomeIcon icon={faKey} />} title={tExt('chat.needsKeyTitle', {})}>
        <Stack gap='xs' align='flex-start'>
          <Text size='sm'>
            {info.access.mode === 'required' ? tExt('chat.needsKeyRequired', {}) : tExt('chat.needsKeyAllowed', {})}
          </Text>
          <Button size='xs' leftSection={<FontAwesomeIcon icon={faKey} />} onClick={() => navigate(ACCOUNT_KEY_PATH)}>
            {tExt('chat.addKey', {})}
          </Button>
        </Stack>
      </Alert>
    );
  }
  if (!info.configured) {
    return <Alert color='yellow'>{tExt('chat.notConfigured', { problem: info.problem ?? '' })}</Alert>;
  }
  return null;
}

function Thinking({ label }: { label: string }) {
  return (
    <AssistantBlock>
      <Group gap='xs' className='h-7'>
        <Loader type='dots' size='sm' />
        <Text size='sm' c='dimmed'>
          {label}
        </Text>
      </Group>
    </AssistantBlock>
  );
}

/**
 * The chat for one room: `base` is the server's or the admin area's assistant API. `onChanged`
 * runs after a confirmed action, so the page around the chat can reload what it shows.
 *
 * `variant` picks the shell: the sidebar page (conversation list beside the chat) or the floating
 * window (the list folds over the chat, `actions` sit in the header). `onStatus` tells the
 * window's button whether an answer is streaming or actions wait while the window is minimised.
 */
export default function Chat({
  base,
  admin,
  onChanged,
  variant = 'page',
  actions,
  onStatus,
}: {
  base: string;
  admin: boolean;
  onChanged?: () => void;
  variant?: 'page' | 'popup';
  actions?: ReactNode;
  onStatus?: (status: { busy: boolean; waiting: boolean }) => void;
}) {
  const navigate = useNavigate();
  const { t: tExt } = useExtTranslations();
  const { addToast } = useToast();
  const queryClient = useQueryClient();

  const info = useQuery({ queryKey: [PACKAGE, base, 'info'], queryFn: () => getInfo(base) });
  const conversations = useQuery({
    queryKey: [PACKAGE, base, 'conversations'],
    queryFn: () => getConversations(base),
  });

  const [selected, setSelected] = useState<string | null>(null);
  const [chat, setChat] = useState<ChatState>(emptyChat);
  const [loading, setLoading] = useState(false);
  const [rollingBack, setRollingBack] = useState(false);
  const [text, setText] = useState('');
  const [listOpen, setListOpen] = useState(false);
  const [following, setFollowing] = useState(true);
  // The turn streaming right now, and the conversation it writes to.
  const stream = useRef<{ conversation: string; controller: AbortController } | null>(null);
  const viewport = useRef<HTMLDivElement>(null);
  const composer = useRef<HTMLTextAreaElement>(null);

  const titles = useMemo(
    () => new Map((info.data?.tools ?? []).map((tool) => [tool.name, tool.title] as const)),
    [info.data],
  );
  const current = conversations.data?.find((conversation) => conversation.uuid === selected);

  const refresh = () => queryClient.invalidateQueries({ queryKey: [PACKAGE, base] });

  const load = (conversation: string) => {
    setLoading(true);
    getConversation(base, conversation)
      .then((detail) => setChat(loadedChat(detail.messages, detail.actions)))
      .catch((error) => addToast(httpErrorToHuman(error), 'error'))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    if (stream.current && stream.current.conversation !== selected) {
      stream.current.controller.abort();
      stream.current = null;
    }
    setFollowing(true);
    if (!selected) {
      setChat(emptyChat);
    } else if (stream.current?.conversation !== selected) {
      load(selected);
    }
  }, [base, selected]);

  const waiting = pendingActions(chat).length > 0;
  useEffect(() => {
    onStatus?.({ busy: chat.busy, waiting });
  }, [chat.busy, waiting]);

  useEffect(() => () => stream.current?.controller.abort(), []);

  // Follow new content only while the user is at the bottom; scrolling up stops it.
  useEffect(() => {
    if (following && viewport.current) {
      viewport.current.scrollTo({ top: viewport.current.scrollHeight });
    }
  }, [chat.messages.length, chat.actions, chat.draft, chat.busy, chat.error, following, loading]);

  const scrollToBottom = () => {
    setFollowing(true);
    viewport.current?.scrollTo({ top: viewport.current.scrollHeight, behavior: 'smooth' });
  };

  const runTurn = async (conversation: string, path: 'messages' | 'continue', body: object) => {
    const controller = new AbortController();
    stream.current = { conversation, controller };
    const isCurrent = () => stream.current?.controller === controller;
    setFollowing(true);
    setChat((state) => ({ ...state, busy: true, error: null, stopped: false, draft: '' }));
    try {
      await streamTurn(
        `${base}/conversations/${conversation}/${path}`,
        body,
        (message) => {
          if (isCurrent()) {
            setChat((state) => applyEvent(state, message));
          }
        },
        controller.signal,
      );
    } catch (error) {
      if (isCurrent()) {
        setChat((state) => ({ ...state, error: error instanceof Error ? error.message : String(error) }));
      }
    } finally {
      if (isCurrent()) {
        stream.current = null;
        setChat((state) => ({ ...state, busy: false, draft: '', running: [] }));
      }
      refresh();
    }
  };

  const send = async (value: string = text) => {
    const message = value.trim();
    if (!message || chat.busy) {
      return;
    }
    let conversation = selected;
    if (!conversation) {
      try {
        conversation = (await createConversation(base)).uuid;
      } catch (error) {
        addToast(httpErrorToHuman(error), 'error');
        return;
      }
      // Claim the stream before the selection changes, so selecting it does not reload it.
      stream.current = { conversation, controller: new AbortController() };
      setSelected(conversation);
    }
    setText('');
    await runTurn(conversation, 'messages', { text: message });
    composer.current?.focus();
  };

  const stop = async () => {
    if (!selected) {
      return;
    }
    try {
      await stopTurn(base, selected);
    } catch (error) {
      addToast(httpErrorToHuman(error), 'error');
    }
  };

  const decide = async (action: Action, confirm: boolean) => {
    if (!selected) {
      return;
    }
    const conversation = selected;
    setChat((state) => ({
      ...state,
      actions: upsertAction(state.actions, { ...action, status: confirm ? 'running' : 'rejected' }),
    }));
    try {
      const decision = await decideAction(base, conversation, action.uuid, confirm);
      setChat((state) =>
        withMessage({ ...state, actions: upsertAction(state.actions, decision.action) }, decision.message),
      );
      if (decision.action.status === 'succeeded') {
        // What the action changed (a name, a file, a backup) shows elsewhere in the panel too.
        queryClient.invalidateQueries();
        onChanged?.();
      }
      if (decision.pending === 0) {
        await runTurn(conversation, 'continue', {});
      }
    } catch (error) {
      addToast(httpErrorToHuman(error), 'error');
      load(conversation);
    }
  };

  const rollback = async (action: Action, force: boolean): Promise<RollbackActionResult | null> => {
    if (!selected) {
      return null;
    }
    const conversation = selected;
    setRollingBack(true);
    try {
      const result = await rollbackAction(base, conversation, action.uuid, force);
      if (result.ok) {
        setChat((state) => ({ ...state, actions: upsertAction(state.actions, result.action) }));
        if (result.action.rollback_status === 'rolled_back') {
          queryClient.invalidateQueries();
          onChanged?.();
          addToast(tExt('action.rollbackSuccess', {}), 'success');
        } else if (result.action.rollback_status === 'failed') {
          addToast(result.action.rollback_result ?? tExt('action.rollbackFailed', {}), 'error');
        }
      }
      return result;
    } catch (error) {
      addToast(httpErrorToHuman(error), 'error');
      load(conversation);
      return null;
    } finally {
      setRollingBack(false);
    }
  };

  const rollbackTo = async (action: Action, force: boolean): Promise<RollbackToResult | null> => {
    if (!selected) {
      return null;
    }
    const conversation = selected;
    setRollingBack(true);
    try {
      const result = await rollbackToAction(base, conversation, action.uuid, force);
      if (result.actions.length > 0) {
        setChat((state) => ({ ...state, actions: upsertActions(state.actions, result.actions) }));
        queryClient.invalidateQueries();
        onChanged?.();
      }
      if (result.ok) {
        const succeededCount = result.actions.filter((item) => item.rollback_status === 'rolled_back').length;
        const failedItem = result.actions.find((item) => item.rollback_status === 'failed');
        if (failedItem) {
          addToast(failedItem.rollback_result ?? tExt('action.rollbackFailed', {}), 'error');
        } else if (result.skipped.length > 0) {
          addToast(
            tExt('action.rollbackToPartial', {
              count: String(succeededCount),
              skipped: String(result.skipped.length),
            }),
            'success',
          );
        } else if (succeededCount > 0) {
          addToast(tExt('action.rollbackToSuccess', { count: String(succeededCount) }), 'success');
        }
      }
      return result;
    } catch (error) {
      addToast(httpErrorToHuman(error), 'error');
      load(conversation);
      return null;
    } finally {
      setRollingBack(false);
    }
  };

  const rename = async (conversation: Conversation, title: string) => {
    try {
      await renameConversation(base, conversation.uuid, title);
      refresh();
    } catch (error) {
      addToast(httpErrorToHuman(error), 'error');
    }
  };

  const remove = async (conversation: Conversation) => {
    await deleteConversation(base, conversation.uuid);
    addToast(tExt('conversations.deleted', {}), 'success');
    if (selected === conversation.uuid) {
      setSelected(null);
    }
    refresh();
  };

  const select = (uuid: string | null) => {
    setSelected(uuid);
    setListOpen(false);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      send();
    }
  };

  const data = info.data;
  const usable = !!data && data.enabled && data.configured;
  const usage = data?.usage;
  const runningTool = chat.running.length > 0 ? chat.running[chat.running.length - 1] : null;
  const runningCall = runningTool
    ? chat.messages
        .flatMap((message) => (message.entry.type === 'assistant' ? message.entry.tool_calls : []))
        .find((call) => call.id === runningTool)
    : undefined;

  const list = (
    <ConversationList
      conversations={conversations.data ?? []}
      loading={conversations.isLoading}
      selected={selected}
      onSelect={select}
      onNew={() => select(null)}
      onRename={rename}
      onDelete={remove}
      disabled={chat.busy}
    />
  );

  const popup = variant === 'popup';

  const card = (
    <Card
      withBorder={!popup}
      p={0}
      radius={popup ? 0 : 'md'}
      h={popup ? '100%' : 'calc(100dvh - 12rem)'}
      mih={popup ? 0 : 480}
      className='flex min-w-0 flex-col overflow-hidden'
    >
      <Group
        justify='space-between'
        wrap='nowrap'
        px='md'
        py='sm'
        className='border-b border-[var(--mantine-color-default-border)]'
      >
        <Group gap='xs' wrap='nowrap' className='min-w-0'>
          <ActionIcon
            variant={popup && listOpen ? 'light' : 'subtle'}
            color='gray'
            className={popup ? undefined : 'lg:hidden!'}
            onClick={() => setListOpen((open) => !open)}
            aria-label={tExt('conversations.title', {})}
            aria-expanded={listOpen}
          >
            <FontAwesomeIcon icon={faBars} />
          </ActionIcon>
          <Text fw={600} truncate='end'>
            {current?.title ?? tExt('conversations.new', {})}
          </Text>
        </Group>
        <Group gap={6} wrap='nowrap' className='shrink-0'>
          {data?.access.source === 'user' && (
            <Tooltip label={tExt('chat.ownKeyHint', {})}>
              <Badge variant='light' color='teal' leftSection={<FontAwesomeIcon icon={faKey} size='xs' />}>
                {tExt('chat.ownKey', {})}
              </Badge>
            </Tooltip>
          )}
          {!popup && data?.configured && (
            <Badge variant='default' className='hidden! sm:inline-flex!' tt='none'>
              {data.model}
            </Badge>
          )}
          {actions}
        </Group>
      </Group>

      {data && (
        <div className='px-4 py-3 empty:hidden'>
          <Notice info={data} />
        </div>
      )}
      {info.error && (
        <Alert color='red' m='md'>
          {httpErrorToHuman(info.error)}
        </Alert>
      )}

      <div className='relative min-h-0 flex-1'>
        {popup && listOpen && (
          <div className='absolute inset-0 z-10 overflow-y-auto bg-[var(--mantine-color-body)] p-3'>{list}</div>
        )}
        <ScrollArea
          h='100%'
          viewportRef={viewport}
          onScrollPositionChange={({ y }) => {
            const element = viewport.current;
            if (element) {
              setFollowing(element.scrollHeight - (y + element.clientHeight) < STICK_DISTANCE);
            }
          }}
        >
          <div className='p-4'>
            {loading ? (
              <Group justify='center' py='xl'>
                <Loader size='sm' />
              </Group>
            ) : (
              <Stack gap='lg'>
                {chat.messages.length === 0 && !chat.busy && (
                  <EmptyState admin={admin} onPick={(prompt) => send(prompt)} disabled={!usable} compact={popup} />
                )}
                <Timeline
                  chat={chat}
                  titles={titles}
                  onDecide={decide}
                  onRollback={rollback}
                  onRollbackTo={rollbackTo}
                  disabled={rollingBack}
                />
                {chat.draft && (
                  <AssistantBlock>
                    <ChatMarkdown text={chat.draft} />
                  </AssistantBlock>
                )}
                {chat.busy && !chat.draft && (
                  <Thinking
                    label={
                      runningCall
                        ? tExt('tool.running', { title: titles.get(runningCall.name) ?? runningCall.name })
                        : tExt('chat.thinking', {})
                    }
                  />
                )}
                {chat.stopped && !chat.busy && (
                  <Text size='sm' c='dimmed' ta='center'>
                    {tExt('chat.stopped', {})}
                  </Text>
                )}
                {chat.error && !chat.busy && (
                  <Alert color='red' title={tExt('chat.errorTitle', {})}>
                    <Stack gap='xs' align='flex-start'>
                      <Text size='sm'>{chat.error}</Text>
                      <Group gap='xs'>
                        {selected && canContinue(chat) && (
                          <Button
                            size='xs'
                            variant='default'
                            leftSection={<FontAwesomeIcon icon={faRotateRight} />}
                            onClick={() => runTurn(selected, 'continue', {})}
                          >
                            {tExt('chat.retry', {})}
                          </Button>
                        )}
                        {data &&
                          data.access.mode !== 'off' &&
                          (data.access.source === 'user' || /\b40[13]\b|key/i.test(chat.error)) && (
                            <Button
                              size='xs'
                              variant='default'
                              onClick={() => navigate(ACCOUNT_KEY_PATH)}
                              leftSection={<FontAwesomeIcon icon={faKey} />}
                            >
                              {tExt('chat.manageKey', {})}
                            </Button>
                          )}
                      </Group>
                    </Stack>
                  </Alert>
                )}
                {selected && chat.stopped && !chat.busy && !chat.error && canContinue(chat) && (
                  <Group justify='center'>
                    <Button
                      size='xs'
                      variant='default'
                      leftSection={<FontAwesomeIcon icon={faRotateRight} />}
                      onClick={() => runTurn(selected, 'continue', {})}
                    >
                      {tExt('chat.retry', {})}
                    </Button>
                  </Group>
                )}
              </Stack>
            )}
          </div>
        </ScrollArea>
        {!following && (
          <ActionIcon
            radius='xl'
            size='lg'
            variant='default'
            className='absolute! bottom-3 left-1/2 -translate-x-1/2 shadow-md'
            onClick={scrollToBottom}
            aria-label={tExt('chat.jumpToLatest', {})}
          >
            <FontAwesomeIcon icon={faArrowDown} />
          </ActionIcon>
        )}
      </div>

      <div className='border-t border-[var(--mantine-color-default-border)] p-3'>
        <Group align='flex-end' gap='xs' wrap='nowrap'>
          <TextArea
            ref={composer}
            className='flex-1'
            placeholder={tExt('chat.placeholder', {})}
            value={text}
            onChange={(event) => setText(event.currentTarget.value)}
            onKeyDown={onKeyDown}
            autosize
            minRows={1}
            maxRows={8}
            maxLength={8000}
            disabled={!usable || chat.busy}
            aria-label={tExt('chat.placeholder', {})}
          />
          {chat.busy ? (
            <Button color='red' variant='light' leftSection={<FontAwesomeIcon icon={faStop} />} onClick={stop}>
              {tExt('chat.stop', {})}
            </Button>
          ) : (
            <Button
              leftSection={<FontAwesomeIcon icon={faPaperPlane} />}
              onClick={() => send()}
              disabled={!usable || !text.trim()}
            >
              {tExt('chat.send', {})}
            </Button>
          )}
        </Group>
        {usage && data?.configured && (
          <Text size='xs' c='dimmed' mt={6} className='tabular-nums'>
            {data.access.source === 'user'
              ? tExt('chat.usageOwnKey', { used: usage.own_key_tokens_today.toLocaleString() })
              : usage.daily_token_limit > 0
                ? tExt('chat.usage', {
                    used: usage.tokens_today.toLocaleString(),
                    limit: usage.daily_token_limit.toLocaleString(),
                  })
                : tExt('chat.usageUnlimited', { used: usage.tokens_today.toLocaleString() })}
            {!popup && (
              <>
                {' · '}
                {tExt('chat.hint', {})}
              </>
            )}
          </Text>
        )}
      </div>
    </Card>
  );

  if (popup) {
    return card;
  }

  return (
    <div className='grid grid-cols-1 gap-4 lg:grid-cols-[17rem_minmax(0,1fr)]'>
      <aside className='hidden lg:block'>{list}</aside>
      <Drawer opened={listOpen} onClose={() => setListOpen(false)} title={tExt('conversations.title', {})} size='xs'>
        {list}
      </Drawer>
      {card}
    </div>
  );
}
