-- A data source is one uploader or import feeding a system. It owns the upload
-- credential issued for it (stored in the management database) and every reading
-- it delivers refers back to it, so connection health and attribution are per
-- source. A removed source is kept as a tombstone: its readings stay attributed.

CREATE TABLE data_sources (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    profile TEXT NOT NULL CHECK (length(trim(profile)) > 0),
    credential_id BLOB CHECK (credential_id IS NULL OR length(credential_id) = 16),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0)
) STRICT;

CREATE UNIQUE INDEX data_sources_live_name_idx
    ON data_sources(system_id, name) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX data_sources_credential_idx
    ON data_sources(credential_id) WHERE credential_id IS NOT NULL;

ALTER TABLE telemetry_hot ADD COLUMN data_source_id BLOB
    CHECK (data_source_id IS NULL OR length(data_source_id) = 16);

CREATE INDEX telemetry_hot_data_source_idx
    ON telemetry_hot(data_source_id, measured_at DESC) WHERE data_source_id IS NOT NULL;
