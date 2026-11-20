import { faPlus, faTrash } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { useState } from 'react';
import ActionIcon from '@/elements/buttons/ActionIcon.tsx';
import Button from '@/elements/buttons/Button.tsx';
import Group from '@/elements/layout/Group.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import ConfirmationModal from '@/elements/modals/ConfirmationModal.tsx';
import Tooltip from '@/elements/overlays/Tooltip.tsx';
import Text from '@/elements/typography/Text.tsx';
import type { Conversation } from '../api.ts';
import { useExtTranslations } from '../translations.ts';

export default function ConversationList({
  conversations,
  selected,
  onSelect,
  onNew,
  onDelete,
  disabled,
}: {
  conversations: Conversation[];
  selected: string | null;
  onSelect: (uuid: string) => void;
  onNew: () => void;
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

      <Button leftSection={<FontAwesomeIcon icon={faPlus} />} onClick={onNew} disabled={disabled} fullWidth>
        {tExt('conversations.new', {})}
      </Button>

      {conversations.length === 0 && (
        <Text size='sm' c='dimmed'>
          {tExt('conversations.empty', {})}
        </Text>
      )}

      {conversations.map((conversation) => (
        <Group
          key={conversation.uuid}
          gap='xs'
          wrap='nowrap'
          className={`rounded-md px-2 py-1 cursor-pointer ${
            conversation.uuid === selected ? 'bg-white/5 light:bg-black/5' : 'hover:bg-white/3 light:hover:bg-black/3'
          }`}
          onClick={() => !disabled && onSelect(conversation.uuid)}
        >
          <div className='min-w-0 flex-1'>
            <Text size='sm' truncate='end'>
              {conversation.title}
            </Text>
            <Text size='xs' c='dimmed'>
              {new Date(conversation.updated).toLocaleString()}
            </Text>
          </div>
          <Tooltip label={tExt('conversations.delete', {})}>
            <ActionIcon
              variant='subtle'
              color='red'
              disabled={disabled}
              onClick={(event) => {
                event.stopPropagation();
                setDeleting(conversation);
              }}
            >
              <FontAwesomeIcon icon={faTrash} />
            </ActionIcon>
          </Tooltip>
        </Group>
      ))}
    </Stack>
  );
}
