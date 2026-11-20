import getServer from '@/api/server/getServer.ts';
import ServerContentContainer from '@/elements/containers/ServerContentContainer.tsx';
import { useServerStore } from '@/stores/server.ts';
import { serverBase } from '../api.ts';
import Chat from '../components/Chat.tsx';
import { useExtTranslations } from '../translations.ts';

export default function ServerAssistantPage() {
  const { t: tExt } = useExtTranslations();
  const uuid = useServerStore((state) => state.server.uuid);
  const setServer = useServerStore((state) => state.setServer);

  // A confirmed action may have renamed the server or changed its variables; the sidebar and
  // header read the server from the store, so load it again.
  const reloadServer = () => {
    getServer(uuid)
      .then(setServer)
      .catch(() => undefined);
  };

  return (
    <ServerContentContainer title={tExt('title', {})} subtitle={tExt('subtitle', {})}>
      <Chat key={uuid} base={serverBase(uuid)} admin={false} onChanged={reloadServer} />
    </ServerContentContainer>
  );
}
