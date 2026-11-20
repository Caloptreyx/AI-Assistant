import AdminContentContainer from '@/elements/containers/AdminContentContainer.tsx';
import { ADMIN_BASE } from '../api.ts';
import Chat from '../components/Chat.tsx';
import { useExtTranslations } from '../translations.ts';

export default function AdminAssistantPage() {
  const { t: tExt } = useExtTranslations();

  return (
    <AdminContentContainer title={tExt('title', {})} subtitle={tExt('adminSubtitle', {})}>
      <Chat base={ADMIN_BASE} admin />
    </AdminContentContainer>
  );
}
