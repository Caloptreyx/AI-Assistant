-- Deterministic action ordering and rollback state tracking.
ALTER TABLE dev_caloptreyx_aiassistant_actions
    ADD COLUMN seq BIGSERIAL NOT NULL,
    ADD COLUMN rollback_status VARCHAR(15) NOT NULL DEFAULT 'unavailable',
    ADD COLUMN rollback_summary TEXT,
    ADD COLUMN rollback_result TEXT,
    ADD COLUMN rolled_back TIMESTAMPTZ;

CREATE INDEX dev_caloptreyx_aiassistant_actions_seq_idx
    ON dev_caloptreyx_aiassistant_actions (conversation_uuid, seq);

-- Pre-change snapshots stored separately so listing actions never transfers large file contents.
CREATE TABLE dev_caloptreyx_aiassistant_snapshots (
    action_uuid uuid PRIMARY KEY
        REFERENCES dev_caloptreyx_aiassistant_actions(uuid) ON DELETE CASCADE,
    kind VARCHAR(31) NOT NULL,
    payload TEXT NOT NULL,
    created TIMESTAMPTZ NOT NULL DEFAULT now()
);
