import { z } from 'zod';
import { axiosInstance, getImpersonatedUser } from '@/api/axios.ts';
import { parseFromApi, serializeForApi } from '@/lib/serialization/api-transform.ts';
import type { Action, Message } from './lib/chat.ts';
import { readSse, type SseMessage } from './lib/sse.ts';

export const PACKAGE = 'dev.caloptreyx.aiassistant';
export const ADMIN_BASE = `/api/admin/extensions/${PACKAGE}`;
export const serverBase = (server: string) => `/api/client/servers/${server}/ai-assistant`;

// Chat payloads keep the backend's snake_case: they are stored and streamed as-is, and tool
// arguments are arbitrary JSON that must not be re-keyed.

export interface ToolInfo {
  name: string;
  title: string;
  confirm: boolean;
}

export interface ChatInfo {
  enabled: boolean;
  configured: boolean;
  problem: string | null;
  provider: 'openai' | 'anthropic';
  model: string;
  tools: ToolInfo[];
  usage: {
    tokens_today: number;
    daily_token_limit: number;
    requests_per_hour: number;
  };
}

export interface Conversation {
  uuid: string;
  title: string;
  created: string;
  updated: string;
}

export interface ConversationDetail {
  conversation: Conversation;
  messages: Message[];
  actions: Action[];
}

export interface Decision {
  action: Action;
  message: Message;
  pending: number;
}

export const getInfo = async (base: string): Promise<ChatInfo> => (await axiosInstance.get(base)).data;

export const getConversations = async (base: string): Promise<Conversation[]> =>
  (await axiosInstance.get(`${base}/conversations`)).data.conversations;

export const createConversation = async (base: string): Promise<Conversation> =>
  (await axiosInstance.post(`${base}/conversations`, {})).data.conversation;

export const getConversation = async (base: string, uuid: string): Promise<ConversationDetail> =>
  (await axiosInstance.get(`${base}/conversations/${uuid}`)).data;

export const deleteConversation = async (base: string, uuid: string): Promise<void> => {
  await axiosInstance.delete(`${base}/conversations/${uuid}`);
};

export const decideAction = async (
  base: string,
  conversation: string,
  action: string,
  confirm: boolean,
): Promise<Decision> =>
  (
    await axiosInstance.post(
      `${base}/conversations/${conversation}/actions/${action}/${confirm ? 'confirm' : 'reject'}`,
      {},
    )
  ).data;

/**
 * Starts a turn (`messages` with `{ text }`, or `continue`) and feeds its server-sent events to
 * `onEvent` until the turn ends. The panel's axios client buffers whole responses, so this uses
 * fetch with the same session cookie and impersonation header.
 */
export async function streamTurn(
  url: string,
  body: object,
  onEvent: (message: SseMessage) => void,
  signal: AbortSignal,
): Promise<void> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    Accept: 'text/event-stream',
  };
  const impersonated = getImpersonatedUser();
  if (impersonated) {
    headers['Calagopus-User'] = impersonated;
  }

  const response = await fetch(url, {
    method: 'POST',
    headers,
    body: JSON.stringify(body),
    credentials: 'same-origin',
    signal,
  });
  if (!response.ok || !response.body) {
    let message = `HTTP ${response.status}`;
    try {
      const data = await response.json();
      if (Array.isArray(data?.errors) && typeof data.errors[0] === 'string') {
        message = data.errors[0];
      }
    } catch {
      // not JSON, keep the status
    }
    if (response.status === 401) {
      window.dispatchEvent(new Event('session-expired'));
    }
    throw new Error(message);
  }
  await readSse(response.body, onEvent);
}

// Settings use the panel's camelCase conversion like the other admin pages.

export const secretSchema = z.object({
  set: z.boolean(),
  hint: z.string().nullable(),
});

export const settingsSchema = z.object({
  provider: z.enum(['openai', 'anthropic']),
  openaiBaseUrl: z.string(),
  openaiApiKey: secretSchema,
  openaiModel: z.string(),
  anthropicApiKey: secretSchema,
  anthropicModel: z.string(),
  serverChatEnabled: z.boolean(),
  adminChatEnabled: z.boolean(),
  requestsPerHour: z.number(),
  dailyTokenLimit: z.number(),
  maxToolSteps: z.number(),
  maxOutputTokens: z.number(),
  toolOutputLimit: z.number(),
  historyLimit: z.number(),
  retentionDays: z.number(),
  instructions: z.string(),
});
export type Settings = z.infer<typeof settingsSchema>;

export const settingsUpdateSchema = settingsSchema.omit({ openaiApiKey: true, anthropicApiKey: true }).extend({
  // undefined keeps the stored key, '' removes it
  openaiApiKey: z.string().optional(),
  anthropicApiKey: z.string().optional(),
});
export type SettingsUpdate = z.infer<typeof settingsUpdateSchema>;

export const getSettings = async (): Promise<Settings> =>
  parseFromApi(settingsSchema, (await axiosInstance.get(`${ADMIN_BASE}/settings`)).data.settings);

export const updateSettings = async (update: SettingsUpdate): Promise<Settings> =>
  parseFromApi(
    settingsSchema,
    (await axiosInstance.put(`${ADMIN_BASE}/settings`, serializeForApi(settingsUpdateSchema, update))).data.settings,
  );

export interface ConnectionTest {
  ok: boolean;
  reply?: string;
  model?: string;
  error?: string;
}

export const testSettings = async (): Promise<ConnectionTest> =>
  (await axiosInstance.post(`${ADMIN_BASE}/settings/test`, {})).data;
