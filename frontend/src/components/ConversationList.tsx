import { faEllipsisVertical, faPen, faPlus, faTrash } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { type KeyboardEvent, useState } from 'react';
import ActionIcon from '@/elements/buttons/ActionIcon.tsx';
import Button from '@/elements/buttons/Button.tsx';
import TextInput from '@/elements/input/TextInput.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import ConfirmationModal from '@/elements/modals/ConfirmationModal.tsx';
import Menu from '@/elements/overlays/Menu.tsx';
import Text from '@/elements/typography/Text.tsx';
import type { Conversation } from '../api.ts';
import { useExtTranslations } from '../translations.ts';

function relativeTime(iso: string): string {
  const seconds = (Date.now() - new Date(iso).getTime()) / 1000;
  const format = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });
  if (seconds < 60) return format.format(0, 'minute');
  if (seconds < 3600) return format.format(-Math.round(seconds / 60), 'minute');
  if (seconds < 86_400) return format.format(-Math.round(seconds / 3600), 'hour');
  if (seconds < 7 * 86_400) return format.format(-Math.round(seconds / 86_400), 'day');
  return new Date(iso).toLocaleDateString();
}

function ConversationRow({
  conversation,
  active,
  disabled,
  onSelect,
  onRename,
  onDelete,
}: {
  conversation: Conversation;
  active: boolean;
  disabled: boolean;
  onSelect: () => void;
  onRename: (title: string) => Promise<void>;
  onDelete: () => void;
}) {
  const { t: tExt } = useExtTranslations();
  const [editing, setEditing] = useState<string | null>(null);

  const save = async () => {
    const title = editing?.trim();
    setEditing(null);
    if (title && title !== conversation.title) {
      await onRename(title);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      save();
    } else if (event.key === 'Escape') {
      setEditing(null);
    }
  };

  if (editing !== null) {
    return (
      <TextInput
        size='xs'
        autoFocus
        value={editing}
        maxLength={255}
        onChange={(event) => setEditing(event.currentTarget.value)}
        onKeyDown={onKeyDown}
        onBlur={save}
        aria-label={tExt('conversations.rename', {})}
      />
    );
  }

  return (
    <div
      role='button'
      tabIndex={0}
      aria-current={active ? 'true' : undefined}
      onClick={() => !disabled && onSelect()}
      onKeyDown={(event) => {
        if (!disabled && (event.key === 'Enter' || event.key === ' ')) {
          event.preventDefault();
          onSelect();
        }
      }}
      className={`group flex cursor-pointer items-center gap-2 rounded-md border border-transparent px-2.5 py-2 transition-colors focus-visible:outline-2 focus-visible:outline-[var(--mantine-primary-color-filled)] ${
        active ? 'bg-[var(--mantine-primary-color-light)]' : 'hover:bg-[var(--mantine-color-default-hover)]'
      } ${disabled ? 'cursor-wait opacity-70' : ''}`}
    >
      <div className='min-w-0 flex-1'>
        <Text size='sm' fw={active ? 600 : 500} truncate='end'>
          {conversation.title}
        </Text>
        <Text size='xs' c='dimmed'>
          {relativeTime(conversation.updated)}
        </Text>
      </div>
      <Menu position='bottom-end' withinPortal>
        <Menu.Target>
          <ActionIcon
            variant='subtle'
            color='gray'
            size='sm'
            className='opacity-60 transition-opacity group-hover:opacity-100'
            onClick={(event) => event.stopPropagation()}
            aria-label={tExt('conversations.options', {})}
          >
            <FontAwesomeIcon icon={faEllipsisVertical} />
          </ActionIcon>
        </Menu.Target>
        <Menu.Dropdown>
          <Menu.Item
            leftSection={<FontAwesomeIcon icon={faPen} size='sm' />}
            onClick={(event) => {
              event.stopPropagation();
              setEditing(conversation.title);
            }}
          >
            {tExt('conversations.rename', {})}
          </Menu.Item>
          <Menu.Item
            color='red'
            leftSection={<FontAwesomeIcon icon={faTrash} size='sm' />}
            disabled={disabled}
            onClick={(event) => {
              event.stopPropagation();
              onDelete();
            }}
          >
            {tExt('conversations.delete', {})}
          </Menu.Item>
        </Menu.Dropdown>
      </Menu>
    </div>
  );
}

export default function ConversationList({
  conversations,
  loading,
  selected,
  onSelect,
  onNew,
  onRename,
  onDelete,
  disabled,
}: {
  conversations: Conversation[];
  loading: boolean;
  selected: string | null;
  onSelect: (uuid: string) => void;
  onNew: () => void;
  onRename: (conversation: Conversation, title: string) => Promise<void>;
  onDelete: (conversation: Conversation) => Promise<void>;
  disabled: boolean;
}) {
  const { t: tExt } = useExtTranslations();
  const [deleting, setDeleting] = useState<Conversation | null>(null);

  return (
    <Stack gap='xs'>
      <ConfirmationModal
        opened={!!deleting}
        onClose={() => setDeleting(null)}
        title={tExt('conversations.deleteTitle', {})}
        confirm={tExt('conversations.delete', {})}
        onConfirmed={async () => {
          if (deleting) {
            await onDelete(deleting);
          }
          setDeleting(null);
        }}
      >
        {tExt('conversations.deleteContent', { title: deleting?.title ?? '' }).md()}
      </ConfirmationModal>

      <Button
        leftSection={<FontAwesomeIcon icon={faPlus} />}
        variant={selected ? 'light' : 'filled'}
        onClick={onNew}
        disabled={disabled}
        fullWidth
      >
        {tExt('conversations.new', {})}
      </Button>

      <Text size='xs' c='dimmed' fw={600} mt='xs' px={4}>
        {tExt('conversations.title', {})}
      </Text>

      {!loading && conversations.length === 0 && (
        <Text size='sm' c='dimmed' px={4}>
          {tExt('conversations.empty', {})}
        </Text>
      )}

      <Stack gap={2}>
        {conversations.map((conversation) => (
          <ConversationRow
            key={conversation.uuid}
            conversation={conversation}
            active={conversation.uuid === selected}
            disabled={disabled}
            onSelect={() => onSelect(conversation.uuid)}
            onRename={(title) => onRename(conversation, title)}
            onDelete={() => setDeleting(conversation)}
          />
        ))}
      </Stack>
    </Stack>
  );
}
