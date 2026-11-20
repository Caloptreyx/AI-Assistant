import ServerContentContainer from '@/elements/containers/ServerContentContainer.tsx';
import { useServerStore } from '@/stores/server.ts';
import { serverBase } from '../api.ts';
import Chat from '../components/Chat.tsx';
import { useExtTranslations } from '../translations.ts';

export default function ServerAssistantPage() {
  const { t: tExt } = useExtTranslations();
  const uuid = useServerStore((state) => state.server.uuid);

  return (
    <ServerContentContainer title={tExt('title', {})} subtitle={tExt('subtitle', {})}>
      <Chat key={uuid} base={serverBase(uuid)} admin={false} />
    </ServerContentContainer>
  );
}
