CREATE TABLE dev_caloptreyx_aiassistant_conversations (
    uuid uuid NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    user_uuid uuid NOT NULL REFERENCES users(uuid) ON DELETE CASCADE,
    -- NULL for conversations in the admin area
    server_uuid uuid REFERENCES servers(uuid) ON DELETE CASCADE,
    title VARCHAR(255) NOT NULL,
    created TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX dev_caloptreyx_aiassistant_conversations_owner_idx
    ON dev_caloptreyx_aiassistant_conversations (user_uuid, server_uuid, updated DESC);
CREATE INDEX dev_caloptreyx_aiassistant_conversations_updated_idx
    ON dev_caloptreyx_aiassistant_conversations (updated);

-- One row per chat entry (user message, assistant message, tool result), as JSON text.
CREATE TABLE dev_caloptreyx_aiassistant_messages (
    id BIGSERIAL PRIMARY KEY,
    conversation_uuid uuid NOT NULL
        REFERENCES dev_caloptreyx_aiassistant_conversations(uuid) ON DELETE CASCADE,
    entry TEXT NOT NULL,
    created TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX dev_caloptreyx_aiassistant_messages_conversation_idx
    ON dev_caloptreyx_aiassistant_messages (conversation_uuid, id);

-- Changes the model proposed. They only run once the user confirms them.
CREATE TABLE dev_caloptreyx_aiassistant_actions (
    uuid uuid NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    conversation_uuid uuid NOT NULL
        REFERENCES dev_caloptreyx_aiassistant_conversations(uuid) ON DELETE CASCADE,
    call_id VARCHAR(255) NOT NULL,
    tool VARCHAR(63) NOT NULL,
    arguments TEXT NOT NULL,
    status VARCHAR(15) NOT NULL DEFAULT 'pending',
    result TEXT,
    created TIMESTAMPTZ NOT NULL DEFAULT now(),
    decided TIMESTAMPTZ
);

CREATE INDEX dev_caloptreyx_aiassistant_actions_conversation_idx
    ON dev_caloptreyx_aiassistant_actions (conversation_uuid, status);

CREATE TABLE dev_caloptreyx_aiassistant_usage (
    user_uuid uuid NOT NULL REFERENCES users(uuid) ON DELETE CASCADE,
    day DATE NOT NULL,
    requests INTEGER NOT NULL DEFAULT 0,
    input_tokens BIGINT NOT NULL DEFAULT 0,
    output_tokens BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (user_uuid, day)
);
