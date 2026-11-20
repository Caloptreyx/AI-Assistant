# AI Assistant

A [Calagopus Panel](https://calagopus.com) extension that adds an AI assistant chat to the panel.
Server users ask it about their own servers, administrators about the whole panel. It reads freely
(status, console, files, backups, schedules, settings, nodes, users) and proposes changes as
**action cards that only run after the user clicks Confirm**.

Package name: `dev.caloptreyx.aiassistant` · Requires panel `>=1.2.10`

## Features

- **Server chat** on every server (**AI Assistant** in the server sidebar) for users with the
  `ai-assistant.chat` server permission. Server owners have it; give it to subusers like any other
  permission. Each chat is bound to its server and cannot reach other servers or the admin API.
- **Admin chat** in the admin area (**Admin → AI Assistant**) for users with the
  `ai-assistant.chat` admin permission: nodes, users and servers overview, server tools for any
  server, and the whole panel API through `search_api` / `describe_api` / `read_api` / `write_api`.
- **Streaming answers** (server-sent events) with Markdown, tool calls shown inline with their
  results.
- **Confirm before act**: every change (power, console commands, writing/deleting files, settings,
  backups, schedules, startup variables, admin API writes) is shown as a card with exactly what will
  be sent. Rejected actions go back to the model as rejected.
- **Providers**: any OpenAI-compatible chat completions API (OpenAI, OpenRouter, Groq, Ollama,
  LM Studio, vLLM, …) or Anthropic. API keys are stored encrypted and never leave the backend.
- **Guardrails**: messages per user per hour, tokens per user per day, model calls per message,
  tool output truncation, history window and conversation retention.
- Conversations are kept per user (and per server) in the panel database.

## Installation

Download `dev_caloptreyx_aiassistant.c7s.zip` from the
[latest release](https://github.com/Caloptreyx/AI-Assistant/releases/latest) and either upload it
under **Admin → Extensions** or drop it into your heavy image's `build/extensions/` directory and
`docker compose restart web`. Extensions require the `:heavy` panel image (or a dev environment).
The migration creates four tables prefixed `dev_caloptreyx_aiassistant_`.

## Setup

Open **Admin → Extensions → AI Assistant** (needs `extensions.manage`):

| Provider | Base URL | Key | Model (example) |
|---|---|---|---|
| OpenAI | `https://api.openai.com/v1` | `sk-…` | `gpt-4o-mini`, `gpt-4.1` |
| OpenRouter | `https://openrouter.ai/api/v1` | `sk-or-…` | `openai/gpt-4o`, `anthropic/claude-sonnet-4.5` |
| Groq | `https://api.groq.com/openai/v1` | `gsk_…` | `llama-3.3-70b-versatile` |
| Ollama | `http://<host>:11434/v1` | none | `qwen3:8b`, `llama3.1` |
| LM Studio | `http://<host>:1234/v1` | none | the loaded model |
| Anthropic | (fixed) | `sk-ant-…` | `claude-sonnet-4-5`, `claude-haiku-4-5` |

The model must support tool (function) calling and streaming. **Test connection** sends a
one-word request with the saved settings. Keys are write-only in the UI: the page only shows
whether a key is stored and its last four characters; leave the field empty to keep it.

Other settings:

| Setting | Default | Meaning |
|---|---|---|
| Server chat / Admin chat | on / on | Where the assistant is offered |
| Messages per user per hour | 30 | `0` = unlimited (panel rate limiter, per user) |
| Tokens per user per day | 500 000 | Input + output as the provider reports them (estimated at four characters per token when it reports none); `0` = unlimited |
| Model calls per message | 8 | Tool rounds before the assistant stops and reports |
| Maximum answer length | 4096 | `max_tokens` for Anthropic |
| Tool output limit | 12 000 characters | Console logs, files and API responses are cut (start and end kept) before the model sees them |
| History sent to the model | 60 entries | Older entries of long conversations are left out |
| Delete conversations after | 30 days | `0` keeps them |
| Extra instructions | empty | Appended to the built-in system prompt |

## Permissions and confirmation

The assistant can never do more than the user could do by hand:

1. **Every tool is a sequence of requests to the panel's own API, sent through the panel's router
   as the chatting user** (the same mechanism as [Cal MCP](https://github.com/Caloptreyx/Cal-MCP),
   whose tools this extension reuses). The panel's permission manager, subuser permissions, admin
   checks, suspension, rate limits and activity log apply to every request exactly as for the
   browser. While an administrator impersonates a user, requests carry the impersonation header so
   the panel's impersonation rules apply too.
2. **Tools the user lacks permissions for are not offered** to the model (for example `read_file`
   needs `files.read-content`, `list_users` needs the `users.read` admin permission) and are refused
   again if the model calls them anyway.
3. **Changes need confirmation.** A changing tool call is stored as a pending action and shown as a
   card; nothing is sent to the panel until the user clicks Confirm. Runs without a confirmation can
   only send GET requests (and not the GETs that hand out console/upload/node credentials), so even
   a misclassified tool cannot change anything. Confirming moves the action from `pending` to
   `running` atomically, so it runs at most once, and re-checks the user's permissions first.
4. **A server chat stays on its server**: every request must target
   `/api/client/servers/<its uuid>/…`, whatever the model asks for.
5. The assistant cannot call its own endpoints.

The confirmation is per action. When the user writes a new message instead of deciding, pending
actions are rejected and the model is told so.

## Tools

| Tool | Chat | Reads / changes | Permission checked before offering |
|---|---|---|---|
| `get_server` | server, admin | read | – |
| `read_console` | server, admin | read | `control.read-console` |
| `list_files`, `read_file` | server, admin | read | `files.read`, `files.read-content` |
| `list_backups`, `list_schedules`, `list_startup_variables` | server, admin | read | `backups.read`, `schedules.read`, `startup.read` |
| `power_server` | server, admin | change | `control.start`/`stop`/`restart` |
| `send_command`, `run_command` | server, admin | change | `control.console` (+ `control.read-console`) |
| `write_file`, `edit_file`, `delete_files` | server, admin | change | `files.create`, `files.delete` |
| `create_backup`, `restore_backup`, `delete_backup` | server, admin | change | `backups.create`/`restore`/`delete` |
| `create_schedule`, `run_schedule` | server, admin | change | `schedules.create`, `schedules.update` |
| `update_startup_variables`, `rename_server` | server, admin | change | `startup.update`, `settings.rename` |
| `panel_stats`, `list_nodes`, `get_node_overview`, `list_users`, `list_all_servers` | admin | read | `stats.read`, `nodes.read`, `users.read`, `servers.read` |
| `list_servers`, `search_api`, `describe_api`, `read_api` | admin | read | – (the panel checks each endpoint) |
| `write_api` | admin | change | – (the panel checks each endpoint) |

In the admin chat, server permissions are left to the panel, server by server.

## Privacy and safety

Conversation text and tool output (console lines, file contents, API responses) are sent to the
configured provider. Use a local model (Ollama, LM Studio) if that data must not leave your
infrastructure. API keys are stored encrypted with the panel's database key.

Tool output is untrusted input to the model: a player's chat line or a file could contain
instructions aimed at it. That is one reason every change waits for a human: read the action card
before you confirm it.

## API

Server chat: `/api/client/servers/{server}/ai-assistant`, admin chat:
`/api/admin/extensions/dev.caloptreyx.aiassistant`. Both offer

| Method | Path | |
|---|---|---|
| GET | `/` | Provider, model, offered tools, today's usage |
| GET / POST | `/conversations` | List / create |
| GET / DELETE | `/conversations/{conversation}` | Messages and actions / delete |
| POST | `/conversations/{conversation}/messages` | `{ "text": "…" }`, answers with `text/event-stream` |
| POST | `/conversations/{conversation}/continue` | Let the model go on after decisions, `text/event-stream` |
| POST | `/conversations/{conversation}/actions/{action}/confirm` | Run a pending action |
| POST | `/conversations/{conversation}/actions/{action}/reject` | `{ "reason": "…" }` optional |

Stream events: `message` (a stored entry), `delta` (answer text), `tool` (a read tool started),
`action` (a proposed change), `error`, `done` (`{ "waiting": true }` while actions wait).
Admin only: `GET/PUT /settings`, `POST /settings/test`.

## Development

Crate at the repository root, frontend in `frontend/`, migrations in `migrations/`. CI
(`.github/workflows/check.yml`) builds the extension inside the newest panel release: typecheck,
oxlint/oxfmt, `pnpm build`, the node tests in `tests/` and `cargo test`. The Rust tests cover the
permission gating and confirmation gate of the tools (against a recording fake panel) and the
provider request bodies and stream parsing against recorded OpenAI, Ollama and Anthropic streams.

Releases: tag `vX.Y.Z` and publish a GitHub release; `release.yml` attaches
`dev_caloptreyx_aiassistant.c7s.zip`. Keep `Cargo.toml`'s version in sync with the tag.

## License

MIT
