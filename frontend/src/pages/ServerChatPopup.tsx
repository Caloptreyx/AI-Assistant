import { useServerCan } from '@/plugins/usePermissions.ts';
import { useServerStore } from '@/stores/server.ts';
import { serverBase } from '../api.ts';
import ChatPopup from '../components/ChatPopup.tsx';
import { useServerReload } from './useServerReload.ts';

/** The floating chat on a server's pages, for users allowed to use the server chat. */
export default function ServerChatPopup() {
  const uuid = useServerStore((state) => state.server.uuid);
  const allowed = useServerCan('ai-assistant.chat');
  const reloadServer = useServerReload();

  if (!uuid || !allowed) {
    return null;
  }
  return <ChatPopup key={uuid} base={serverBase(uuid)} admin={false} onChanged={reloadServer} />;
}
