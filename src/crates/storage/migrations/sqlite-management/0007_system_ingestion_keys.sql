-- Stores one-time-display ingestion credentials bound to a single registered system.
CREATE UNIQUE INDEX system_registry_account_system_key
    ON system_registry(account_id, system_id);

CREATE TABLE system_ingestion_keys (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    account_id BLOB NOT NULL REFERENCES accounts(id) ON DELETE CASCADE CHECK (length(account_id) = 16),
    system_id BLOB NOT NULL CHECK (length(system_id) = 16),
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    lookup_prefix TEXT NOT NULL UNIQUE CHECK (
        length(lookup_prefix) = 32
        AND lookup_prefix NOT GLOB '*[^0-9a-f]*'
    ),
    credential_digest BLOB NOT NULL CHECK (length(credential_digest) = 32),
    hash_key_version INTEGER NOT NULL CHECK (hash_key_version > 0),
    rotated_from BLOB REFERENCES system_ingestion_keys(id) ON DELETE SET NULL CHECK (
        rotated_from IS NULL OR length(rotated_from) = 16
    ),
    created_at INTEGER NOT NULL,
    overlap_expires_at INTEGER,
    expires_at INTEGER,
    revoked_at INTEGER,
    last_used_at INTEGER,
    UNIQUE (credential_digest),
    CHECK (rotated_from IS NULL OR rotated_from <> id),
    CHECK (overlap_expires_at IS NULL OR overlap_expires_at > created_at),
    CHECK (expires_at IS NULL OR expires_at > created_at),
    CHECK (revoked_at IS NULL OR revoked_at >= created_at),
    CHECK (last_used_at IS NULL OR last_used_at >= created_at),
    FOREIGN KEY (account_id, system_id)
        REFERENCES system_registry(account_id, system_id) ON DELETE CASCADE
) STRICT;

CREATE INDEX system_ingestion_keys_system_lifecycle_idx
    ON system_ingestion_keys(account_id, system_id, revoked_at, expires_at, created_at DESC);

CREATE INDEX system_ingestion_keys_rotation_idx
    ON system_ingestion_keys(rotated_from, overlap_expires_at);
