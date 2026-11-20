// The chat as the browser sees it: the stored conversation plus what the running turn streams.
// Kept free of React and panel imports so it can be tested with `node --test`.

import type { SseMessage } from './sse.ts';

export interface ToolCall {
  id: string;
  name: string;
  arguments: unknown;
}

export type Entry =
  | { type: 'user'; text: string }
  | { type: 'assistant'; text: string; tool_calls: ToolCall[] }
  | { type: 'tool_result'; call_id: string; name: string; content: string; is_error: boolean };

export interface Message {
  id: number;
  entry: Entry;
  created: string;
}

export type ActionStatus = 'pending' | 'running' | 'succeeded' | 'failed' | 'rejected';

export interface Action {
  uuid: string;
  call_id: string;
  tool: string;
  title: string;
  arguments: unknown;
  status: ActionStatus;
  result: string | null;
  created: string;
  decided: string | null;
}

export interface ChatState {
  messages: Message[];
  actions: Action[];
  /** Text of the answer the model is writing right now. */
  draft: string;
  /** Read tools that started and have no result yet. */
  running: string[];
  /** A turn is streaming. */
  busy: boolean;
  error: string | null;
}

export const emptyChat: ChatState = {
  messages: [],
  actions: [],
  draft: '',
  running: [],
  busy: false,
  error: null,
};

export function loadedChat(messages: Message[], actions: Action[]): ChatState {
  return { ...emptyChat, messages, actions };
}

function upsertMessage(messages: Message[], message: Message): Message[] {
  return messages.some((existing) => existing.id === message.id)
    ? messages.map((existing) => (existing.id === message.id ? message : existing))
    : [...messages, message].sort((a, b) => a.id - b.id);
}

export function upsertAction(actions: Action[], action: Action): Action[] {
  return actions.some((existing) => existing.uuid === action.uuid)
    ? actions.map((existing) => (existing.uuid === action.uuid ? action : existing))
    : [...actions, action];
}

/** Applies a stored message (from the stream or a decision) to the chat. */
export function withMessage(state: ChatState, message: Message): ChatState {
  const entry = message.entry;
  return {
    ...state,
    messages: upsertMessage(state.messages, message),
    draft: entry.type === 'assistant' ? '' : state.draft,
    running: entry.type === 'tool_result' ? state.running.filter((id) => id !== entry.call_id) : state.running,
  };
}

/** Applies one event of a streaming turn. Unknown or broken events are ignored. */
export function applyEvent(state: ChatState, message: SseMessage): ChatState {
  let data: Record<string, unknown>;
  try {
    data = JSON.parse(message.data);
  } catch {
    return state;
  }

  switch (message.event) {
    case 'delta':
      return { ...state, draft: state.draft + String(data.text ?? '') };
    case 'message':
      return withMessage(state, data as unknown as Message);
    case 'tool':
      return { ...state, running: [...state.running, String(data.call_id)] };
    case 'action': {
      const action = data as unknown as Action;
      return {
        ...state,
        actions: upsertAction(state.actions, action),
        running: state.running.filter((id) => id !== action.call_id),
      };
    }
    case 'error':
      return { ...state, error: String(data.message ?? 'Unknown error') };
    case 'done':
      return { ...state, busy: false, draft: '', running: [] };
    default:
      return state;
  }
}

export type CallView =
  | { kind: 'action'; call: ToolCall; action: Action }
  | { kind: 'result'; call: ToolCall; content: string; isError: boolean }
  | { kind: 'running'; call: ToolCall }
  | { kind: 'missing'; call: ToolCall };

/** What to show for one tool call: its action card, its result, or that it is still running. */
export function callView(state: ChatState, call: ToolCall): CallView {
  const action = state.actions.find((candidate) => candidate.call_id === call.id);
  if (action) {
    return { kind: 'action', call, action };
  }
  for (const message of state.messages) {
    if (message.entry.type === 'tool_result' && message.entry.call_id === call.id) {
      return { kind: 'result', call, content: message.entry.content, isError: message.entry.is_error };
    }
  }
  return state.running.includes(call.id) ? { kind: 'running', call } : { kind: 'missing', call };
}

export function pendingActions(state: ChatState): Action[] {
  return state.actions.filter((action) => action.status === 'pending');
}

/** The model has something to answer: the last entry is the user's or a tool result. */
export function canContinue(state: ChatState): boolean {
  const last = state.messages[state.messages.length - 1];
  return (
    !state.busy &&
    pendingActions(state).length === 0 &&
    !!last &&
    (last.entry.type === 'user' || last.entry.type === 'tool_result')
  );
}
