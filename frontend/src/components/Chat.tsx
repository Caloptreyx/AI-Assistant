import { faPaperPlane, faRotateRight } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { type KeyboardEvent, useEffect, useMemo, useRef, useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import Button from '@/elements/buttons/Button.tsx';
import Badge from '@/elements/data-display/Badge.tsx';
import Card from '@/elements/data-display/Card.tsx';
import Alert from '@/elements/feedback/Alert.tsx';
import Spinner from '@/elements/feedback/Spinner.tsx';
import TextArea from '@/elements/input/TextArea.tsx';
import Group from '@/elements/layout/Group.tsx';
import ScrollArea from '@/elements/layout/ScrollArea.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import Text from '@/elements/typography/Text.tsx';
import { useToast } from '@/providers/ToastProvider.tsx';
import {
  type Conversation,
  createConversation,
  decideAction,
  deleteConversation,
  getConversation,
  getConversations,
  getInfo,
  PACKAGE,
  streamTurn,
} from '../api.ts';
import {
  type Action,
  applyEvent,
  type ChatState,
  canContinue,
  emptyChat,
  loadedChat,
  upsertAction,
  withMessage,
} from '../lib/chat.ts';
import { useExtTranslations } from '../translations.ts';
import ConversationList from './ConversationList.tsx';
import Timeline, { AssistantText } from './Timeline.tsx';

/** The chat for one room: `base` is the server's or the admin area's assistant API. */
export default function Chat({ base, admin }: { base: string; admin: boolean }) {
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
  const [text, setText] = useState('');
  // The turn streaming right now, and the conversation it writes to.
  const stream = useRef<{ conversation: string; controller: AbortController } | null>(null);
  const bottom = useRef<HTMLDivElement>(null);

  const titles = useMemo(
    () => new Map((info.data?.tools ?? []).map((tool) => [tool.name, tool.title] as const)),
    [info.data],
  );

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
    if (!selected) {
      setChat(emptyChat);
    } else if (stream.current?.conversation !== selected) {
      load(selected);
    }
  }, [base, selected]);

  useEffect(() => () => stream.current?.controller.abort(), []);

  useEffect(() => {
    bottom.current?.scrollIntoView({ block: 'end' });
  }, [chat.messages.length, chat.actions, chat.draft, chat.busy]);

  const runTurn = async (conversation: string, path: 'messages' | 'continue', body: object) => {
    const controller = new AbortController();
    stream.current = { conversation, controller };
    const current = () => stream.current?.controller === controller;
    setChat((state) => ({ ...state, busy: true, error: null, draft: '' }));
    try {
      await streamTurn(
        `${base}/conversations/${conversation}/${path}`,
        body,
        (message) => {
          if (current()) {
            setChat((state) => applyEvent(state, message));
          }
        },
        controller.signal,
      );
    } catch (error) {
      if (current()) {
        setChat((state) => ({ ...state, error: error instanceof Error ? error.message : String(error) }));
      }
    } finally {
      if (current()) {
        stream.current = null;
        setChat((state) => ({ ...state, busy: false, draft: '', running: [] }));
      }
      refresh();
    }
  };

  const send = async () => {
    const message = text.trim();
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
      if (decision.pending === 0) {
        await runTurn(conversation, 'continue', {});
      }
    } catch (error) {
      addToast(httpErrorToHuman(error), 'error');
      load(conversation);
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

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      send();
    }
  };

  const data = info.data;
  const usable = !!data && data.enabled && data.configured;
  const usage = data?.usage;

  return (
    <div className='grid grid-cols-1 gap-4 lg:grid-cols-[16rem_1fr]'>
      <ConversationList
        conversations={conversations.data ?? []}
        selected={selected}
        onSelect={setSelected}
        onNew={() => setSelected(null)}
        onDelete={remove}
        disabled={chat.busy}
      />

      <Card withBorder p='md' className='min-w-0'>
        <Stack gap='sm'>
          {data && (
            <Group gap='xs'>
              <Badge variant='light'>{tExt('chat.model', { model: data.model })}</Badge>
              {usage && (
                <Text size='xs' c='dimmed'>
                  {usage.daily_token_limit > 0
                    ? tExt('chat.usage', {
                        used: usage.tokens_today.toLocaleString(),
                        limit: usage.daily_token_limit.toLocaleString(),
                      })
                    : tExt('chat.usageUnlimited', { used: usage.tokens_today.toLocaleString() })}
                </Text>
              )}
            </Group>
          )}
          {info.error && <Alert color='red'>{httpErrorToHuman(info.error)}</Alert>}
          {data && !data.enabled && <Alert color='yellow'>{tExt('chat.disabled', {})}</Alert>}
          {data && data.enabled && !data.configured && (
            <Alert color='yellow'>{tExt('chat.notConfigured', { problem: data.problem ?? '' })}</Alert>
          )}

          <ScrollArea h='calc(100vh - 22rem)' mih={320} offsetScrollbars>
            {loading ? (
              <Group justify='center' py='xl'>
                <Spinner />
              </Group>
            ) : (
              <Stack gap='md' pr='xs'>
                {chat.messages.length === 0 && !chat.busy && (
                  <Text c='dimmed'>{admin ? tExt('chat.adminWelcome', {}) : tExt('chat.welcome', {})}</Text>
                )}
                <Timeline chat={chat} titles={titles} onDecide={decide} />
                {chat.draft && <AssistantText text={chat.draft} />}
                {chat.busy && !chat.draft && (
                  <Group gap='xs'>
                    <Spinner size={16} />
                    <Text size='sm' c='dimmed'>
                      {tExt('chat.thinking', {})}
                    </Text>
                  </Group>
                )}
                {chat.error && <Alert color='red'>{chat.error}</Alert>}
                {selected && chat.error && canContinue(chat) && (
                  <Group>
                    <Button
                      variant='default'
                      leftSection={<FontAwesomeIcon icon={faRotateRight} />}
                      onClick={() => runTurn(selected, 'continue', {})}
                    >
                      {tExt('chat.continue', {})}
                    </Button>
                  </Group>
                )}
                <div ref={bottom} />
              </Stack>
            )}
          </ScrollArea>

          <Group align='flex-end' wrap='nowrap'>
            <TextArea
              className='flex-1'
              placeholder={tExt('chat.placeholder', {})}
              value={text}
              onChange={(event) => setText(event.currentTarget.value)}
              onKeyDown={onKeyDown}
              autosize
              minRows={1}
              maxRows={8}
              maxLength={8000}
              disabled={!usable}
            />
            <Button
              leftSection={<FontAwesomeIcon icon={faPaperPlane} />}
              onClick={send}
              loading={chat.busy}
              disabled={!usable || !text.trim()}
            >
              {tExt('chat.send', {})}
            </Button>
          </Group>
        </Stack>
      </Card>
    </div>
  );
}
