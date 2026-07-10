CREATE TABLE account_projection_inbox (
    account_id BLOB NOT NULL REFERENCES accounts(id) ON DELETE CASCADE CHECK (length(account_id) = 16),
    source_sequence INTEGER NOT NULL CHECK (source_sequence > 0),
    event_id BLOB NOT NULL CHECK (length(event_id) = 16),
    event_kind TEXT NOT NULL CHECK (event_kind IN ('upsert', 'invalidate', 'delete')),
    system_id BLOB NOT NULL CHECK (length(system_id) = 16),
    payload_hash BLOB NOT NULL CHECK (length(payload_hash) = 32),
    received_at INTEGER NOT NULL,
    applied_at INTEGER NOT NULL,
    PRIMARY KEY (account_id, source_sequence),
    UNIQUE (account_id, event_id)
) STRICT;

CREATE TRIGGER account_projection_inbox_no_update
BEFORE UPDATE ON account_projection_inbox
BEGIN
    SELECT RAISE(ABORT, 'projection inbox records are append-only');
END;

CREATE TRIGGER account_projection_inbox_no_delete
BEFORE DELETE ON account_projection_inbox
BEGIN
    SELECT RAISE(ABORT, 'projection inbox records are append-only');
END;

CREATE TABLE projection_invalidation_reservations (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    account_id BLOB NOT NULL REFERENCES accounts(id) ON DELETE CASCADE CHECK (length(account_id) = 16),
    system_id BLOB NOT NULL CHECK (length(system_id) = 16),
    reason TEXT NOT NULL CHECK (reason IN ('visibility_reduction', 'system_deletion', 'account_suspension', 'operator_repair')),
    reserved_at INTEGER NOT NULL,
    resolved_sequence INTEGER CHECK (resolved_sequence IS NULL OR resolved_sequence > 0),
    resolved_at INTEGER,
    UNIQUE (account_id, system_id, id),
    CHECK ((resolved_sequence IS NULL) = (resolved_at IS NULL))
) STRICT;

CREATE INDEX projection_invalidation_pending_idx
    ON projection_invalidation_reservations(account_id, reserved_at, system_id)
    WHERE resolved_sequence IS NULL;
