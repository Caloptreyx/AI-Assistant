DROP TABLE IF EXISTS dev_caloptreyx_aiassistant_snapshots;

DROP INDEX IF EXISTS dev_caloptreyx_aiassistant_actions_seq_idx;

ALTER TABLE dev_caloptreyx_aiassistant_actions
    DROP COLUMN IF EXISTS rolled_back,
    DROP COLUMN IF EXISTS rollback_result,
    DROP COLUMN IF EXISTS rollback_summary,
    DROP COLUMN IF EXISTS rollback_status,
    DROP COLUMN IF EXISTS seq;
