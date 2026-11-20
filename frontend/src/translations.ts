import { defineTranslations } from 'shared';

const translations = defineTranslations({
  items: {},
  translations: {
    title: 'AI Assistant',
    subtitle: 'Ask questions and let the assistant act for you. Every change waits for your confirmation.',
    adminSubtitle: 'Ask about the whole panel. Every change waits for your confirmation.',
    conversations: {
      title: 'Conversations',
      new: 'New chat',
      empty: 'No conversations yet.',
      options: 'Conversation options',
      rename: 'Rename',
      delete: 'Delete',
      deleteTitle: 'Delete Conversation',
      deleteContent: 'Delete **{title}** and its history?',
      deleted: 'Conversation deleted.',
    },
    empty: {
      title: 'What can I help with on this server?',
      adminTitle: 'What can I help with in the panel?',
      body: 'I can read logs, files, settings and backups, and propose changes. Nothing changes until you confirm it.',
    },
    suggestions: {
      server1: 'Why did my server crash last time?',
      server2: 'Show me the last 50 console lines',
      server3: 'Which startup variables can I change?',
      server4: 'List my backups and how much space they use',
      admin1: 'Give me an overview of the panel',
      admin2: 'Which nodes are there and how full are they?',
      admin3: 'Show the newest users',
      admin4: 'Which servers are suspended?',
    },
    chat: {
      placeholder: 'Ask the assistant…',
      hint: 'Enter to send, Shift+Enter for a new line',
      send: 'Send',
      stop: 'Stop',
      stopped: 'Stopped.',
      retry: 'Retry',
      thinking: 'Thinking…',
      jumpToLatest: 'Jump to the latest message',
      errorTitle: 'Something went wrong',
      disabled: 'The administrator turned the AI assistant off here.',
      notConfigured: 'The AI assistant is not set up yet: {problem}',
      needsKeyTitle: 'Add your API key',
      needsKeyRequired: 'This panel uses your own AI provider key for the assistant. Add one to start chatting.',
      needsKeyAllowed: 'The panel has no AI provider set up. You can still use the assistant with your own key.',
      addKey: 'Add your key',
      manageKey: 'Check your key',
      ownKey: 'Your key',
      ownKeyHint: 'Requests go to your own provider account; the daily token limit does not apply.',
      usage: '{used} of {limit} tokens used today',
      usageUnlimited: '{used} tokens used today',
      usageOwnKey: '{used} tokens on your key today',
    },
    markdown: {
      copy: 'Copy',
      copied: 'Copied',
    },
    tool: {
      running: 'Running {title}…',
    },
    action: {
      pending: 'Needs confirmation',
      running: 'Running',
      succeeded: 'Done',
      failed: 'Failed',
      rejected: 'Rejected',
      confirm: 'Confirm',
      confirmDanger: 'Confirm anyway',
      reject: 'Reject',
      danger: 'This deletes or overwrites data, or interrupts players, and cannot simply be undone.',
      raw: 'Raw arguments',
      result: 'Result',
    },
    account: {
      title: 'AI Assistant',
      subtitle: 'Use your own AI provider key for the assistant.',
      off: 'The administrator does not allow own API keys on this panel.',
      required: 'This panel requires your own key: the assistant only works once you add one.',
      allowed:
        'Optional: with your own key, your requests go to your provider account and the daily token limit does not apply. Without one, the panel’s key is used.',
      current: 'Your key',
      updated: 'Last changed {date}',
      add: 'Add a key',
      change: 'Change your key',
      provider: 'Provider',
      custom: 'Custom (OpenAI-compatible)',
      baseUrlDescription:
        'The API root that /chat/completions is appended to. It must be a public address; local and private networks are blocked.',
      keyDescription: 'Stored encrypted on the panel and never shown again, not even to you.',
      keepKey: 'Leave empty to keep {hint}',
      save: 'Save',
      saved: 'Your key is saved.',
      remove: 'Remove key',
      removeTitle: 'Remove API Key',
      removeRequired: 'Remove your key? You cannot use the assistant on this panel until you add one again.',
      removeAllowed: 'Remove your key? The assistant uses the panel’s key again.',
      removed: 'Your key was removed.',
    },
    settings: {
      provider: {
        title: 'Provider',
        kind: 'Provider',
        openai: 'OpenAI-compatible (OpenAI, OpenRouter, Groq, Ollama, LM Studio, …)',
        anthropic: 'Anthropic',
        baseUrl: 'Base URL',
        baseUrlDescription:
          'The API root that /chat/completions is appended to, e.g. https://api.openai.com/v1, https://openrouter.ai/api/v1 or http://localhost:11434/v1.',
        apiKey: 'API key',
        apiKeyDescription:
          'Stored encrypted on the panel and never sent to browsers. Leave empty to keep the current key.',
        apiKeyStored: 'A key is stored ({hint}).',
        apiKeyStoredNoHint: 'A key is stored.',
        apiKeyNone: 'No key stored.',
        clearKey: 'Remove the stored key',
        model: 'Model',
        modelDescription:
          'The model name the provider expects, e.g. gpt-4o-mini, openai/gpt-4o or claude-sonnet-4-5. It must support tool calling.',
        test: 'Test connection',
        testOk: 'The provider answered: {reply}',
        testFailed: 'Test failed: {error}',
      },
      access: {
        title: 'Where the assistant is available',
        serverChat: 'Server chat',
        serverChatDescription:
          'A chat page on every server for users with the ai-assistant.chat server permission (server owners have it).',
        adminChat: 'Admin chat',
        adminChatDescription: 'A chat page in the admin area for users with the ai-assistant.chat admin permission.',
      },
      userKeys: {
        title: 'User API keys',
        mode: 'Own keys (bring your own key)',
        off: 'Off',
        allowed: 'Allowed',
        required: 'Required',
        offDescription: 'Everyone uses the panel’s key above; users cannot add their own.',
        allowedDescription:
          'Users may add their own key under Account → AI Assistant. It is used instead of the panel’s key; users without one use the panel’s key.',
        requiredDescription:
          'The panel’s key is never used for chats. Users must add their own key before they can chat.',
        customUrls: 'Allow custom base URLs for user keys',
        customUrlsDescription:
          'By default users pick a known provider (OpenAI, Anthropic, OpenRouter, Groq, Mistral, DeepSeek). With this on they may enter any OpenAI-compatible URL; the panel then only connects to public addresses (no localhost, private, link-local or cloud metadata addresses, no redirects).',
      },
      limits: {
        title: 'Limits',
        requestsPerHour: 'Messages per user per hour',
        requestsPerHourDescription: 'Counts every message, also with the user’s own key. 0 means no limit.',
        dailyTokenLimit: 'Tokens per user per day on the panel’s key',
        dailyTokenLimitDescription:
          'Input plus output tokens paid by the panel’s key. Users with their own key are not limited by this. 0 means no limit.',
        maxToolSteps: 'Model calls per message',
        maxToolStepsDescription: 'How many tool rounds the assistant may take before it stops and reports.',
        maxOutputTokens: 'Maximum answer length (tokens)',
        maxOutputTokensDescription:
          'Sent to Anthropic as max_tokens; OpenAI-compatible providers use their own default.',
        toolOutputLimit: 'Tool output limit (characters)',
        toolOutputLimitDescription:
          'Console logs, files and API responses are cut to this length before the model sees them.',
        historyLimit: 'History sent to the model (entries)',
        historyLimitDescription: 'Older entries of long conversations are left out.',
        retentionDays: 'Delete conversations after (days)',
        retentionDaysDescription: 'Conversations untouched for this long are deleted. 0 keeps them.',
      },
      prompt: {
        title: 'Instructions',
        instructions: 'Extra instructions for the assistant',
        instructionsDescription: 'Appended to the built-in system prompt, e.g. house rules or which games you host.',
      },
      save: 'Save',
      saved: 'Settings saved.',
    },
  },
});

export const useExtTranslations = translations.useTranslations.bind(translations);
export const getExtTranslations = translations.getTranslations.bind(translations);

export default translations;
