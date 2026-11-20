DELETE FROM dev_caloptreyx_aiassistant_usage WHERE source <> 'panel';
ALTER TABLE dev_caloptreyx_aiassistant_usage
    DROP CONSTRAINT dev_caloptreyx_aiassistant_usage_pkey;
ALTER TABLE dev_caloptreyx_aiassistant_usage
    ADD PRIMARY KEY (user_uuid, day);
ALTER TABLE dev_caloptreyx_aiassistant_usage DROP COLUMN source;

DROP TABLE IF EXISTS dev_caloptreyx_aiassistant_user_keys;
