import getServer from '@/api/server/getServer.ts';
import { useServerStore } from '@/stores/server.ts';

/**
 * Loads the current server again. A confirmed action may have renamed it or changed its
 * variables, and the sidebar and header read the server from the store.
 */
export function useServerReload(): () => void {
  const uuid = useServerStore((state) => state.server.uuid);
  const setServer = useServerStore((state) => state.setServer);

  return () => {
    getServer(uuid)
      .then(setServer)
      .catch(() => undefined);
  };
}
