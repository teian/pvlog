-- A data source is one uploader or import feeding a system. It owns the upload
-- credential issued for it and every reading it delivers refers back to it, so
-- connection health and attribution are per source. A removed source is kept as
-- a tombstone: its readings stay attributed.

CREATE TABLE account_data.data_sources (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    system_id UUID NOT NULL,
    name TEXT NOT NULL CHECK (length(btrim(name)) > 0),
    profile TEXT NOT NULL CHECK (length(btrim(profile)) > 0),
    credential_id UUID,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    deleted_at BIGINT,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    PRIMARY KEY (account_id, id),
    FOREIGN KEY (account_id, system_id)
        REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX data_sources_live_name_idx
    ON account_data.data_sources(account_id, system_id, name) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX data_sources_credential_idx
    ON account_data.data_sources(credential_id) WHERE credential_id IS NOT NULL;

ALTER TABLE telemetry.hot_observations ADD COLUMN data_source_id UUID;

CREATE INDEX hot_observations_data_source_idx
    ON telemetry.hot_observations(account_id, data_source_id, measured_at DESC)
    WHERE data_source_id IS NOT NULL;
