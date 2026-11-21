import { faMinus, faRobot, faXmark } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { Indicator, Loader, Paper, Portal } from '@mantine/core';
import { useQuery } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { useLocation } from 'react-router';
import ActionIcon from '@/elements/buttons/ActionIcon.tsx';
import Tooltip from '@/elements/overlays/Tooltip.tsx';
import { getInfo, PACKAGE } from '../api.ts';
import { isChatPage } from '../lib/display.ts';
import { useExtTranslations } from '../translations.ts';
import Chat from './Chat.tsx';

// Below Mantine's modals and drawers (200), so confirmations and the panel's own dialogs stay on
// top, and below the panel's toasts (999) and upload card (998), which may sit bottom-right too.
const Z_INDEX = 190;
// Above Mint's phone bottom bar when it is on; the variable is unset everywhere else.
const BOTTOM = 'calc(1rem + var(--nebula-bottom-nav-h, 0px))';

/**
 * The floating chat: a button in the bottom-right corner that opens the same chat as the page in a
 * window over the panel. Mounted once around the server's (or admin area's) routes, so the window
 * keeps its conversation, and a running answer, while the user moves between pages.
 */
export default function ChatPopup({
  base,
  admin,
  onChanged,
}: {
  base: string;
  admin: boolean;
  onChanged?: () => void;
}) {
  const { t: tExt } = useExtTranslations();
  const location = useLocation();
  const info = useQuery({ queryKey: [PACKAGE, base, 'info'], queryFn: () => getInfo(base) });

  const [open, setOpen] = useState(false);
  // The chat stays mounted while minimised; closing drops it, so the next open starts fresh.
  const [session, setSession] = useState<number | null>(null);
  const [status, setStatus] = useState({ busy: false, waiting: false });

  // The chat page shows the chat already; the window steps aside there and comes back after.
  const onChatPage = isChatPage(location.pathname);

  useEffect(() => {
    if (onChatPage) {
      setOpen(false);
    }
  }, [onChatPage]);

  if (!info.data?.enabled) {
    return null;
  }

  const show = () => {
    setSession((current) => current ?? Date.now());
    setOpen(true);
  };

  const close = () => {
    setOpen(false);
    setSession(null);
    setStatus({ busy: false, waiting: false });
  };

  const title = tExt('title', {});

  return (
    <Portal>
      {session !== null && (
        <Paper
          role='dialog'
          aria-label={title}
          withBorder
          shadow='xl'
          className='fixed flex flex-col overflow-hidden max-sm:inset-0! max-sm:h-dvh! max-sm:w-screen! max-sm:rounded-none!'
          style={{
            display: open ? undefined : 'none',
            zIndex: Z_INDEX,
            right: '1rem',
            bottom: BOTTOM,
            width: 'min(28rem, calc(100vw - 2rem))',
            height: 'min(42rem, calc(100dvh - 2rem - var(--nebula-bottom-nav-h, 0px)))',
            borderRadius: 'var(--mantine-radius-lg)',
          }}
        >
          <Chat
            key={session}
            base={base}
            admin={admin}
            onChanged={onChanged}
            variant='popup'
            onStatus={setStatus}
            actions={
              <>
                <Tooltip label={tExt('popup.minimise', {})}>
                  <ActionIcon
                    variant='subtle'
                    color='gray'
                    onClick={() => setOpen(false)}
                    aria-label={tExt('popup.minimise', {})}
                  >
                    <FontAwesomeIcon icon={faMinus} />
                  </ActionIcon>
                </Tooltip>
                <Tooltip label={tExt('popup.close', {})}>
                  <ActionIcon variant='subtle' color='gray' onClick={close} aria-label={tExt('popup.close', {})}>
                    <FontAwesomeIcon icon={faXmark} />
                  </ActionIcon>
                </Tooltip>
              </>
            }
          />
        </Paper>
      )}

      {!open && !onChatPage && (
        <div className='fixed print:hidden' style={{ zIndex: Z_INDEX, right: '1rem', bottom: BOTTOM }}>
          <Tooltip label={status.waiting ? tExt('popup.waiting', {}) : title} position='left'>
            <Indicator
              disabled={!status.busy && !status.waiting}
              color={status.waiting ? 'yellow' : 'blue'}
              processing={status.busy}
              size={14}
              offset={6}
              withBorder
            >
              <ActionIcon
                size={56}
                radius='xl'
                variant='filled'
                className='shadow-lg'
                onClick={show}
                aria-label={tExt('popup.open', {})}
                aria-haspopup='dialog'
              >
                {status.busy ? <Loader size={22} color='white' /> : <FontAwesomeIcon icon={faRobot} size='lg' />}
              </ActionIcon>
            </Indicator>
          </Tooltip>
        </div>
      )}
    </Portal>
  );
}
