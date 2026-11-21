// Where the chat is offered, from the `chat_display` setting the backend renders into index.html.
// Kept free of React and panel imports so it can be tested with `node --test`.

export const CHAT_DISPLAYS = ['page', 'popup', 'both'] as const;

export type ChatDisplay = (typeof CHAT_DISPLAYS)[number];

/** Which shells to mount. Anything unexpected (an older backend, no variable) means the page. */
export function chatShells(value: unknown): { page: boolean; popup: boolean } {
  const display = CHAT_DISPLAYS.find((known) => known === value) ?? 'page';
  return { page: display !== 'popup', popup: display !== 'page' };
}

/** The floating window stays out of the way on the chat's own page, which already shows the chat. */
export function isChatPage(pathname: string): boolean {
  return /\/ai-assistant\/?$/.test(pathname);
}
