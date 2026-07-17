-- Stores one-time-display ingestion credentials bound to a single registered system.
CREATE UNIQUE INDEX system_registry_account_system_key
    ON management.system_registry(account_id, system_id);

CREATE TABLE management.system_ingestion_keys (
    id UUID PRIMARY KEY,
    account_id UUID NOT NULL REFERENCES management.accounts(id) ON DELETE CASCADE,
    system_id UUID NOT NULL,
    name TEXT NOT NULL CHECK (length(btrim(name)) > 0),
    lookup_prefix TEXT NOT NULL UNIQUE CHECK (
        lookup_prefix ~ '^[0-9a-f]{32}$'
    ),
    credential_digest BYTEA NOT NULL UNIQUE CHECK (octet_length(credential_digest) = 32),
    hash_key_version BIGINT NOT NULL CHECK (hash_key_version > 0),
    rotated_from UUID REFERENCES management.system_ingestion_keys(id) ON DELETE SET NULL,
    created_at BIGINT NOT NULL,
    overlap_expires_at BIGINT,
    expires_at BIGINT,
    revoked_at BIGINT,
    last_used_at BIGINT,
    CHECK (rotated_from IS NULL OR rotated_from <> id),
    CHECK (overlap_expires_at IS NULL OR overlap_expires_at > created_at),
    CHECK (expires_at IS NULL OR expires_at > created_at),
    CHECK (revoked_at IS NULL OR revoked_at >= created_at),
    CHECK (last_used_at IS NULL OR last_used_at >= created_at),
    FOREIGN KEY (account_id, system_id)
        REFERENCES management.system_registry(account_id, system_id) ON DELETE CASCADE
);

CREATE INDEX system_ingestion_keys_system_lifecycle_idx
    ON management.system_ingestion_keys(
        account_id,
        system_id,
        revoked_at,
        expires_at,
        created_at DESC
    );

CREATE INDEX system_ingestion_keys_rotation_idx
    ON management.system_ingestion_keys(rotated_from, overlap_expires_at);
