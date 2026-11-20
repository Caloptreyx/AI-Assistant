-- Users' own provider keys, encrypted with the panel's database key.
CREATE TABLE dev_caloptreyx_aiassistant_user_keys (
    user_uuid uuid NOT NULL PRIMARY KEY REFERENCES users(uuid) ON DELETE CASCADE,
    preset VARCHAR(31) NOT NULL,
    -- only for the `custom` preset
    base_url TEXT,
    model VARCHAR(255) NOT NULL,
    api_key_encrypted TEXT NOT NULL,
    created TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Usage is kept per key that paid: `panel` (counts against the daily limit) or `user`.
ALTER TABLE dev_caloptreyx_aiassistant_usage
    ADD COLUMN source VARCHAR(15) NOT NULL DEFAULT 'panel';
ALTER TABLE dev_caloptreyx_aiassistant_usage
    DROP CONSTRAINT dev_caloptreyx_aiassistant_usage_pkey;
ALTER TABLE dev_caloptreyx_aiassistant_usage
    ADD PRIMARY KEY (user_uuid, day, source);
