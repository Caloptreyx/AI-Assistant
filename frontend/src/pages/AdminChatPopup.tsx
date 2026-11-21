import { useAdminCan } from '@/plugins/usePermissions.ts';
import { ADMIN_BASE } from '../api.ts';
import ChatPopup from '../components/ChatPopup.tsx';

/** The floating chat in the admin area, for users with the admin chat permission. */
export default function AdminChatPopup() {
  const allowed = useAdminCan('ai-assistant.chat');

  if (!allowed) {
    return null;
  }
  return <ChatPopup base={ADMIN_BASE} admin />;
}
