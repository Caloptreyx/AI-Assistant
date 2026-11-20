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
      delete: 'Delete conversation',
      deleteTitle: 'Delete Conversation',
      deleteContent: 'Delete **{title}** and its history?',
      deleted: 'Conversation deleted.',
    },
    chat: {
      placeholder: 'Ask the assistant… (Enter to send, Shift+Enter for a new line)',
      send: 'Send',
      continue: 'Continue',
      thinking: 'Thinking…',
      welcome: 'Ask anything about this server, for example "Why did my server crash?" or "Set the max players to 50".',
      adminWelcome:
        'Ask anything about the panel, for example "Which nodes are low on disk?" or "Suspend the server Survival".',
      disabled: 'The administrator turned the AI assistant off here.',
      notConfigured: 'The AI assistant is not set up yet: {problem}',
      model: 'Model: {model}',
      usage: '{used} of {limit} tokens used today',
      usageUnlimited: '{used} tokens used today',
      you: 'You',
      assistant: 'Assistant',
    },
    tool: {
      running: 'Running {title}…',
      done: '{title}',
      failed: '{title} failed',
      missing: '{title}: no result',
      showResult: 'Show result',
      hideResult: 'Hide result',
    },
    action: {
      pending: 'Waiting for your confirmation',
      running: 'Running…',
      succeeded: 'Done',
      failed: 'Failed',
      rejected: 'Rejected',
      confirm: 'Confirm',
      reject: 'Reject',
      details: 'What will be sent',
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
      limits: {
        title: 'Limits',
        requestsPerHour: 'Messages per user per hour',
        requestsPerHourDescription: '0 means no limit.',
        dailyTokenLimit: 'Tokens per user per day',
        dailyTokenLimitDescription: 'Input plus output tokens. 0 means no limit.',
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
